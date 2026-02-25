use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::tools::{Tool, ToolDefinition, ToolResult};

// ── Google OAuth2 token management ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoogleTokens {
    access_token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

fn tokens_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("google_tokens.json")
}

fn load_tokens(data_dir: &PathBuf) -> Option<GoogleTokens> {
    let path = tokens_path(data_dir);
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_tokens(data_dir: &PathBuf, tokens: &GoogleTokens) -> Result<()> {
    let path = tokens_path(data_dir);
    let content = serde_json::to_string_pretty(tokens)?;
    std::fs::write(&path, content)?;
    Ok(())
}

/// Perform the OAuth2 authorization flow: open browser → get code → exchange for tokens.
pub async fn authorize_google(
    client_id: &str,
    client_secret: &str,
    redirect_port: u16,
    data_dir: &PathBuf,
) -> Result<()> {
    let redirect_uri = format!("http://localhost:{}", redirect_port);
    let scope = "https://www.googleapis.com/auth/calendar";

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?\
         client_id={}&\
         redirect_uri={}&\
         response_type=code&\
         scope={}&\
         access_type=offline&\
         prompt=consent",
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(scope),
    );

    println!("\n📋 Opening browser for Google Calendar authorization...");
    println!("   If the browser doesn't open, visit this URL:\n");
    println!("   {}\n", auth_url);

    let _ = open::that(&auth_url);

    // Start local server to receive the callback
    let listener = TcpListener::bind(format!("127.0.0.1:{}", redirect_port))
        .await
        .with_context(|| format!("Failed to bind to port {}", redirect_port))?;

    println!("⏳ Waiting for authorization callback on port {}...", redirect_port);

    let (mut socket, _) = listener.accept().await?;
    let mut buf = vec![0u8; 4096];
    let n = socket.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]).to_string();

    // Extract the authorization code from the request
    let code = extract_code_from_request(&request)
        .context("Failed to extract authorization code from callback")?;

    // Send success response to browser
    let response_html = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body><h2>✅ GMV Agent authorized successfully!</h2>\
        <p>You can close this tab and return to the terminal.</p></body></html>";
    socket.write_all(response_html.as_bytes()).await?;
    drop(socket);

    // Exchange code for tokens
    let client = Client::new();
    let token_response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", &redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .context("Failed to exchange authorization code for tokens")?;

    let status = token_response.status();
    let body: Value = token_response.json().await?;

    if !status.is_success() {
        anyhow::bail!(
            "Token exchange failed ({}): {}",
            status,
            serde_json::to_string_pretty(&body)?
        );
    }

    let access_token = body["access_token"]
        .as_str()
        .context("Missing access_token")?
        .to_string();
    let refresh_token = body["refresh_token"]
        .as_str()
        .context("Missing refresh_token")?
        .to_string();
    let expires_in = body["expires_in"].as_i64().unwrap_or(3600);

    let tokens = GoogleTokens {
        access_token,
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(expires_in),
    };

    save_tokens(data_dir, &tokens)?;
    println!("✅ Google Calendar authorized and tokens saved!\n");

    Ok(())
}

fn extract_code_from_request(request: &str) -> Option<String> {
    // Parse "GET /callback?code=xxx&... HTTP/1.1" or "GET /?code=xxx..."
    let first_line = request.lines().next()?;
    let path = first_line.split_whitespace().nth(1)?;

    // Find the query string
    let query = path.split('?').nth(1)?;

    // Parse query parameters
    for param in query.split('&') {
        let mut parts = param.splitn(2, '=');
        let key = parts.next()?;
        let value = parts.next()?;
        if key == "code" {
            return Some(urlencoding::decode(value).ok()?.into_owned());
        }
    }
    None
}

/// Get a valid access token, refreshing if needed.
async fn get_access_token(
    data_dir: &PathBuf,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    let mut tokens = load_tokens(data_dir).context(
        "Google Calendar not authorized. Run 'gmv-agent setup google-calendar' first.",
    )?;

    // Check if token is expired (with 60s buffer)
    if Utc::now() >= tokens.expires_at - chrono::Duration::seconds(60) {
        // Refresh the token
        let client = Client::new();
        let resp = client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("refresh_token", &tokens.refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("Failed to refresh Google token")?;

        let body: Value = resp.json().await?;
        tokens.access_token = body["access_token"]
            .as_str()
            .context("Failed to get refreshed access_token")?
            .to_string();
        let expires_in = body["expires_in"].as_i64().unwrap_or(3600);
        tokens.expires_at = Utc::now() + chrono::Duration::seconds(expires_in);

        save_tokens(data_dir, &tokens)?;
    }

    Ok(tokens.access_token)
}

// ── Calendar config passed to tools ──────────────────────────────────

#[derive(Debug, Clone)]
pub struct CalendarConfig {
    pub data_dir: PathBuf,
    pub client_id: String,
    pub client_secret: String,
}

// ════════════════════════════════════════════════════════════════════
//  CreateCalendarEvent
// ════════════════════════════════════════════════════════════════════

pub struct CreateCalendarEvent {
    config: CalendarConfig,
    client: Client,
}

impl CreateCalendarEvent {
    pub fn new(config: CalendarConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for CreateCalendarEvent {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_calendar_event".into(),
            description: "Create a new event in Google Calendar.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Event title/summary"
                    },
                    "start_time": {
                        "type": "string",
                        "description": "Start time in ISO 8601 format (e.g., 2026-02-26T10:00:00-05:00)"
                    },
                    "end_time": {
                        "type": "string",
                        "description": "End time in ISO 8601 format"
                    },
                    "description": {
                        "type": "string",
                        "description": "Event description (optional)"
                    },
                    "location": {
                        "type": "string",
                        "description": "Event location (optional)"
                    }
                },
                "required": ["title", "start_time", "end_time"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let title = params["title"]
            .as_str()
            .context("Missing 'title'")?;
        let start_time = params["start_time"]
            .as_str()
            .context("Missing 'start_time'")?;
        let end_time = params["end_time"]
            .as_str()
            .context("Missing 'end_time'")?;
        let description = params["description"].as_str().unwrap_or("");
        let location = params["location"].as_str().unwrap_or("");

        let token = get_access_token(
            &self.config.data_dir,
            &self.config.client_id,
            &self.config.client_secret,
        )
        .await?;

        let event_body = json!({
            "summary": title,
            "description": description,
            "location": location,
            "start": {
                "dateTime": start_time,
            },
            "end": {
                "dateTime": end_time,
            }
        });

        let resp = self
            .client
            .post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
            .bearer_auth(&token)
            .json(&event_body)
            .send()
            .await
            .context("Failed to create calendar event")?;

        let status = resp.status();
        let body: Value = resp.json().await?;

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "Failed to create event: {}",
                body.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
            )));
        }

        let event_id = body["id"].as_str().unwrap_or("unknown");
        let html_link = body["htmlLink"].as_str().unwrap_or("");

        Ok(ToolResult::ok(format!(
            "Event created successfully!\nTitle: {}\nStart: {}\nEnd: {}\nID: {}\nLink: {}",
            title, start_time, end_time, event_id, html_link
        )))
    }
}

// ════════════════════════════════════════════════════════════════════
//  ListCalendarEvents
// ════════════════════════════════════════════════════════════════════

pub struct ListCalendarEvents {
    config: CalendarConfig,
    client: Client,
}

impl ListCalendarEvents {
    pub fn new(config: CalendarConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for ListCalendarEvents {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_calendar_events".into(),
            description: "List upcoming events from Google Calendar.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of events to return (default: 10)"
                    },
                    "date": {
                        "type": "string",
                        "description": "List events for this date (YYYY-MM-DD). Defaults to today."
                    }
                }
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let max_results = params["max_results"].as_u64().unwrap_or(10);

        let token = get_access_token(
            &self.config.data_dir,
            &self.config.client_id,
            &self.config.client_secret,
        )
        .await?;

        // Determine time range
        let (time_min, time_max) = if let Some(date_str) = params["date"].as_str() {
            let date_min = format!("{}T00:00:00Z", date_str);
            let date_max = format!("{}T23:59:59Z", date_str);
            (date_min, Some(date_max))
        } else {
            (Utc::now().to_rfc3339(), None)
        };

        let mut url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/primary/events?\
             timeMin={}&maxResults={}&singleEvents=true&orderBy=startTime",
            urlencoding::encode(&time_min),
            max_results
        );

        if let Some(ref tmax) = time_max {
            url.push_str(&format!("&timeMax={}", urlencoding::encode(tmax)));
        }

        let resp = self
            .client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Failed to list calendar events")?;

        let status = resp.status();
        let body: Value = resp.json().await?;

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "Failed to list events: {}",
                body.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
            )));
        }

        let items = body["items"].as_array();
        let events = match items {
            Some(arr) if !arr.is_empty() => arr,
            _ => return Ok(ToolResult::ok("No upcoming events found.")),
        };

        let mut output = format!("Found {} event(s):\n\n", events.len());
        for event in events {
            let summary = event["summary"].as_str().unwrap_or("(No title)");
            let start = event["start"]["dateTime"]
                .as_str()
                .or_else(|| event["start"]["date"].as_str())
                .unwrap_or("Unknown");
            let end = event["end"]["dateTime"]
                .as_str()
                .or_else(|| event["end"]["date"].as_str())
                .unwrap_or("Unknown");
            let location = event["location"].as_str().unwrap_or("");
            let meet_link = event["hangoutLink"].as_str().unwrap_or("");

            output.push_str(&format!("📅 {}\n", summary));
            output.push_str(&format!("   Start: {}\n", start));
            output.push_str(&format!("   End:   {}\n", end));
            if !location.is_empty() {
                output.push_str(&format!("   Location: {}\n", location));
            }
            if !meet_link.is_empty() {
                output.push_str(&format!("   Meet: {}\n", meet_link));
            }
            output.push('\n');
        }

        Ok(ToolResult::ok(output))
    }
}

// ════════════════════════════════════════════════════════════════════
//  CreateMeeting
// ════════════════════════════════════════════════════════════════════

pub struct CreateMeeting {
    config: CalendarConfig,
    client: Client,
}

impl CreateMeeting {
    pub fn new(config: CalendarConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for CreateMeeting {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_meeting".into(),
            description: "Create a meeting in Google Calendar with attendees and a Google Meet link."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Meeting title"
                    },
                    "start_time": {
                        "type": "string",
                        "description": "Start time in ISO 8601 format"
                    },
                    "end_time": {
                        "type": "string",
                        "description": "End time in ISO 8601 format"
                    },
                    "attendees": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "List of attendee email addresses"
                    },
                    "description": {
                        "type": "string",
                        "description": "Meeting description/agenda (optional)"
                    }
                },
                "required": ["title", "start_time", "end_time", "attendees"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let title = params["title"]
            .as_str()
            .context("Missing 'title'")?;
        let start_time = params["start_time"]
            .as_str()
            .context("Missing 'start_time'")?;
        let end_time = params["end_time"]
            .as_str()
            .context("Missing 'end_time'")?;
        let description = params["description"].as_str().unwrap_or("");

        let attendees: Vec<Value> = params["attendees"]
            .as_array()
            .context("Missing 'attendees'")?
            .iter()
            .filter_map(|v| v.as_str())
            .map(|email| json!({"email": email}))
            .collect();

        let token = get_access_token(
            &self.config.data_dir,
            &self.config.client_id,
            &self.config.client_secret,
        )
        .await?;

        let event_body = json!({
            "summary": title,
            "description": description,
            "start": {
                "dateTime": start_time,
            },
            "end": {
                "dateTime": end_time,
            },
            "attendees": attendees,
            "conferenceData": {
                "createRequest": {
                    "requestId": uuid::Uuid::new_v4().to_string(),
                    "conferenceSolutionKey": {
                        "type": "hangoutsMeet"
                    }
                }
            }
        });

        let resp = self
            .client
            .post(
                "https://www.googleapis.com/calendar/v3/calendars/primary/events\
                 ?conferenceDataVersion=1",
            )
            .bearer_auth(&token)
            .json(&event_body)
            .send()
            .await
            .context("Failed to create meeting")?;

        let status = resp.status();
        let body: Value = resp.json().await?;

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "Failed to create meeting: {}",
                body.get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("Unknown error")
            )));
        }

        let event_id = body["id"].as_str().unwrap_or("unknown");
        let html_link = body["htmlLink"].as_str().unwrap_or("");
        let meet_link = body["hangoutLink"].as_str().unwrap_or("(not generated)");

        let attendee_list: Vec<&str> = params["attendees"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        Ok(ToolResult::ok(format!(
            "Meeting created successfully!\n\
             Title: {}\n\
             Start: {}\n\
             End: {}\n\
             Attendees: {}\n\
             Google Meet: {}\n\
             ID: {}\n\
             Link: {}",
            title,
            start_time,
            end_time,
            attendee_list.join(", "),
            meet_link,
            event_id,
            html_link
        )))
    }
}
