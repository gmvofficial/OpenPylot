use anyhow::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Web search tool using Brave Search (HTML scraping, no API key required).
pub struct WebSearchTool {
    client: Client,
}

impl WebSearchTool {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web for information. Returns search results with titles, URLs, and snippets.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let query = params
            .get("query")
            .and_then(|q| q.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

        let max_results = params
            .get("max_results")
            .and_then(|m| m.as_u64())
            .unwrap_or(5) as usize;

        // Use Brave Search (no API key required, no CAPTCHA)
        let url = format!(
            "https://search.brave.com/search?q={}&source=web",
            urlencoding::encode(query)
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "text/html")
            .header("Accept-Language", "en-US,en;q=0.9")
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Web search request failed: {}", e))?;

        let html = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read search response: {}", e))?;

        // Parse results from Brave Search HTML
        let mut results = Vec::new();
        let document = scraper::Html::parse_document(&html);

        // Brave uses .snippet for result containers
        let snippet_selector = scraper::Selector::parse(".snippet").unwrap_or_else(|_| {
            scraper::Selector::parse("div").unwrap()
        });
        let title_selector = scraper::Selector::parse(".search-snippet-title, .title").unwrap_or_else(|_| {
            scraper::Selector::parse("a").unwrap()
        });
        let desc_selector = scraper::Selector::parse(".snippet-content, .snippet-description").unwrap_or_else(|_| {
            scraper::Selector::parse("p").unwrap()
        });
        let url_selector = scraper::Selector::parse(".snippet-url, a[href]").unwrap_or_else(|_| {
            scraper::Selector::parse("a").unwrap()
        });

        for snippet_el in document.select(&snippet_selector).take(max_results) {
            let title = snippet_el
                .select(&title_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let snippet_text = snippet_el
                .select(&desc_selector)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let result_url = snippet_el
                .select(&url_selector)
                .find_map(|el| {
                    el.value().attr("href").and_then(|h| {
                        if h.starts_with("http") { Some(h.to_string()) } else { None }
                    })
                })
                .unwrap_or_default();

            if !title.is_empty() && title.len() > 3 {
                results.push(format!(
                    "{}. **{}**\n   URL: {}\n   {}",
                    results.len() + 1,
                    title,
                    result_url,
                    snippet_text
                ));
            }
        }

        if results.is_empty() {
            Ok(ToolResult::ok(format!(
                "No results found for query: '{}'",
                query
            )))
        } else {
            Ok(ToolResult::ok(format!(
                "Search results for '{}':\n\n{}",
                query,
                results.join("\n\n")
            )))
        }
    }
}

/// Web page content extraction tool.
/// Inspired by Hermes' web_extract with article extraction.
pub struct WebExtractTool {
    client: Client,
}

impl WebExtractTool {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// Basic SSRF protection: block requests to internal/private IPs.
    fn is_safe_url(url: &str) -> bool {
        let url_lower = url.to_lowercase();
        // Block obvious internal targets
        if url_lower.contains("localhost")
            || url_lower.contains("127.0.0.1")
            || url_lower.contains("0.0.0.0")
            || url_lower.contains("[::1]")
            || url_lower.contains("169.254.")
            || url_lower.contains("10.")
            || url_lower.contains("192.168.")
            || url_lower.starts_with("file://")
            || url_lower.starts_with("ftp://")
        {
            return false;
        }
        // Must start with http:// or https://
        url_lower.starts_with("http://") || url_lower.starts_with("https://")
    }
}

#[async_trait]
impl Tool for WebExtractTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "web_extract".to_string(),
            description: "Extract the main text content from a web page URL. Useful for reading articles, documentation, and web pages.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL of the web page to extract content from"
                    },
                    "max_length": {
                        "type": "integer",
                        "description": "Maximum characters to return (default: 5000)",
                        "default": 5000
                    }
                },
                "required": ["url"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let url = params
            .get("url")
            .and_then(|u| u.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;

        let max_length = params
            .get("max_length")
            .and_then(|m| m.as_u64())
            .unwrap_or(5000) as usize;

        // SSRF protection
        if !Self::is_safe_url(url) {
            return Ok(ToolResult::err(
                "URL blocked: only public HTTP/HTTPS URLs are allowed",
            ));
        }

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch URL: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "HTTP error {}: could not fetch URL",
                status
            )));
        }

        let html = response
            .text()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read page content: {}", e))?;

        // Extract text content using scraper
        let document = scraper::Html::parse_document(&html);

        // Try to get main content areas first
        let content_selectors = ["article", "main", "[role=main]", ".content", "#content", ".post-content", ".entry-content"];
        let mut text = String::new();

        for selector_str in &content_selectors {
            if let Ok(selector) = scraper::Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let element_text: String = element.text().collect::<Vec<_>>().join(" ");
                    let cleaned = element_text
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if cleaned.len() > 100 {
                        text = cleaned;
                        break;
                    }
                }
                if !text.is_empty() {
                    break;
                }
            }
        }

        // Fallback: extract all body text
        if text.is_empty() {
            if let Ok(body_sel) = scraper::Selector::parse("body") {
                for element in document.select(&body_sel) {
                    text = element
                        .text()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                }
            }
        }

        // Extract title
        let title = if let Ok(title_sel) = scraper::Selector::parse("title") {
            document
                .select(&title_sel)
                .next()
                .map(|el| el.text().collect::<String>().trim().to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };

        if text.is_empty() {
            return Ok(ToolResult::ok("Could not extract meaningful content from the page."));
        }

        // Truncate to max_length
        let truncated = if text.len() > max_length {
            format!("{}... [truncated]", &text[..max_length])
        } else {
            text
        };

        let result = if title.is_empty() {
            format!("Content from {}:\n\n{}", url, truncated)
        } else {
            format!("**{}**\nSource: {}\n\n{}", title, url, truncated)
        };

        Ok(ToolResult::ok(result))
    }
}
