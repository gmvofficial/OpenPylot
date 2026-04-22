use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

/// Embedded job scheduler for the OpenPylot.
///
/// Runs periodic tasks like calendar sync, RSVP monitoring,
/// meeting reminders, daily briefings, and reminder checks.
pub struct AgentScheduler {
    jobs: Vec<ScheduledJob>,
    state_path: PathBuf,
    state: SchedulerState,
}

/// A single scheduled job with cron expression and handler.
pub struct ScheduledJob {
    pub name: String,
    pub description: String,
    pub cron_expr: String,
    pub schedule: Schedule,
    pub enabled: bool,
    pub handler: Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>> + Send + Sync>,
}

/// Persistent state tracking when jobs last ran.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerState {
    pub jobs: Vec<JobState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobState {
    pub name: String,
    pub last_run: Option<DateTime<Utc>>,
    pub last_result: Option<String>,
    pub run_count: u64,
    pub error_count: u64,
}

/// Notification channel trait for sending alerts.
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, message: &str) -> Result<()>;
}

/// Terminal-only notifier (prints to stdout).
pub struct TerminalNotifier;

#[async_trait::async_trait]
impl Notifier for TerminalNotifier {
    async fn send(&self, message: &str) -> Result<()> {
        println!(
            "\n{} {}",
            "📢 [Notification]".bright_yellow(),
            message
        );
        Ok(())
    }
}

/// Telegram notifier — sends messages via Telegram Bot API.
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    client: reqwest::Client,
}

impl TelegramNotifier {
    pub fn new(bot_token: String, chat_id: String) -> Self {
        Self {
            bot_token,
            chat_id,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, message: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        self.client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": self.chat_id,
                "text": message,
                "parse_mode": "Markdown"
            }))
            .send()
            .await?;
        Ok(())
    }
}

impl AgentScheduler {
    /// Create a new scheduler with state file at the given data directory.
    pub fn new(data_dir: &Path) -> Self {
        let state_path = data_dir.join("scheduler_state.json");
        let state = Self::load_state(&state_path).unwrap_or_default();

        Self {
            jobs: Vec::new(),
            state_path,
            state,
        }
    }

    /// Add a job to the scheduler.
    pub fn add_job(
        &mut self,
        name: &str,
        description: &str,
        cron_expr: &str,
        enabled: bool,
        handler: impl Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send>>
            + Send
            + Sync
            + 'static,
    ) -> Result<()> {
        // Validate cron expression (cron crate expects 7 fields: sec min hour dom month dow year)
        let full_cron = normalize_cron(cron_expr);
        let schedule = Schedule::from_str(&full_cron)
            .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", cron_expr, e))?;

        // Ensure state entry exists
        if !self.state.jobs.iter().any(|j| j.name == name) {
            self.state.jobs.push(JobState {
                name: name.to_string(),
                last_run: None,
                last_result: None,
                run_count: 0,
                error_count: 0,
            });
        }

        self.jobs.push(ScheduledJob {
            name: name.to_string(),
            description: description.to_string(),
            cron_expr: cron_expr.to_string(),
            schedule,
            enabled,
            handler: Box::new(handler),
        });

        Ok(())
    }

    /// Enable or disable a job by name.
    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> bool {
        if let Some(job) = self.jobs.iter_mut().find(|j| j.name == name) {
            job.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// List all jobs with their status and next run time.
    pub fn list_jobs(&self) -> Vec<JobInfo> {
        self.jobs
            .iter()
            .map(|job| {
                let next_run = if job.enabled {
                    job.schedule.upcoming(Utc).next()
                } else {
                    None
                };
                let state = self.state.jobs.iter().find(|s| s.name == job.name);

                JobInfo {
                    name: job.name.clone(),
                    description: job.description.clone(),
                    cron_expr: job.cron_expr.clone(),
                    enabled: job.enabled,
                    next_run,
                    last_run: state.and_then(|s| s.last_run),
                    run_count: state.map(|s| s.run_count).unwrap_or(0),
                    error_count: state.map(|s| s.error_count).unwrap_or(0),
                }
            })
            .collect()
    }

    /// Run a specific job immediately by name.
    pub async fn run_job(&mut self, name: &str) -> Result<String> {
        let job = self
            .jobs
            .iter()
            .find(|j| j.name == name)
            .ok_or_else(|| anyhow::anyhow!("Job not found: {}", name))?;

        let result = (job.handler)().await;

        // Update state
        if let Some(state) = self.state.jobs.iter_mut().find(|s| s.name == name) {
            state.last_run = Some(Utc::now());
            state.run_count += 1;
            match &result {
                Ok(msg) => state.last_result = Some(msg.clone()),
                Err(e) => {
                    state.error_count += 1;
                    state.last_result = Some(format!("Error: {}", e));
                }
            }
        }

        self.save_state()?;
        result
    }

    /// Start the scheduler loop. This runs indefinitely.
    pub async fn start(scheduler: Arc<Mutex<Self>>) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(30));

        tracing::info!("Scheduler started");

        loop {
            ticker.tick().await;

            let mut sched = scheduler.lock().await;
            let now = Utc::now();

            for i in 0..sched.jobs.len() {
                if !sched.jobs[i].enabled {
                    continue;
                }

                // Check if job should run
                let should_run = {
                    let last_run = sched
                        .state
                        .jobs
                        .iter()
                        .find(|s| s.name == sched.jobs[i].name)
                        .and_then(|s| s.last_run);

                    match last_run {
                        Some(last) => {
                            // Find the next scheduled time after the last run
                            if let Some(next) = sched.jobs[i].schedule.after(&last).next() {
                                next <= now
                            } else {
                                false
                            }
                        }
                        None => {
                            // Never run before — check if we're past the first scheduled time
                            true
                        }
                    }
                };

                if should_run {
                    let job_name = sched.jobs[i].name.clone();
                    tracing::info!("Running scheduled job: {}", job_name);

                    let result = (sched.jobs[i].handler)().await;

                    if let Some(state) = sched
                        .state
                        .jobs
                        .iter_mut()
                        .find(|s| s.name == job_name)
                    {
                        state.last_run = Some(Utc::now());
                        state.run_count += 1;
                        match &result {
                            Ok(msg) => {
                                tracing::info!("Job {} completed: {}", job_name, msg);
                                state.last_result = Some(msg.clone());
                            }
                            Err(e) => {
                                tracing::error!("Job {} failed: {}", job_name, e);
                                state.error_count += 1;
                                state.last_result = Some(format!("Error: {}", e));
                            }
                        }
                    }

                    let _ = sched.save_state();
                }
            }
        }
    }

    // ── State persistence ────────────────────────────────────────────

    fn load_state(path: &Path) -> Result<SchedulerState> {
        if !path.exists() {
            return Ok(SchedulerState::default());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    fn save_state(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.state)?;
        std::fs::write(&self.state_path, content)?;
        Ok(())
    }
}

/// Information about a scheduled job (for display).
#[derive(Debug, Clone)]
pub struct JobInfo {
    pub name: String,
    pub description: String,
    pub cron_expr: String,
    pub enabled: bool,
    pub next_run: Option<DateTime<Utc>>,
    pub last_run: Option<DateTime<Utc>>,
    pub run_count: u64,
    pub error_count: u64,
}

/// Normalize a 5-field cron expression to the 7-field format expected
/// by the `cron` crate (sec min hour dom month dow year).
pub fn normalize_cron(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    match fields.len() {
        // Standard 5-field: prepend seconds (0) and append year (*)
        5 => format!("0 {} *", expr),
        // 6-field: prepend seconds (0)
        6 => format!("0 {}", expr),
        // Already 7-field
        _ => expr.to_string(),
    }
}

/// Print job list to terminal.
pub fn print_jobs(jobs: &[JobInfo]) {
    if jobs.is_empty() {
        println!("{}", "No scheduled jobs configured.".bright_yellow());
        return;
    }

    println!(
        "{}\n{}",
        "Scheduled Jobs".bright_blue().bold(),
        "─".repeat(70).dimmed()
    );

    for job in jobs {
        let status = if job.enabled {
            "✅ enabled".bright_green().to_string()
        } else {
            "⏸ disabled".dimmed().to_string()
        };

        let next = job
            .next_run
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "—".to_string());

        let last = job
            .last_run
            .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_else(|| "never".to_string());

        println!(
            "  {} [{}]  cron: {}",
            job.name.bright_cyan(),
            status,
            job.cron_expr.dimmed()
        );
        println!(
            "    {} | Last: {} | Runs: {} | Errors: {}",
            format!("Next: {}", next).dimmed(),
            last.dimmed(),
            job.run_count.to_string().dimmed(),
            if job.error_count > 0 {
                job.error_count.to_string().bright_red().to_string()
            } else {
                job.error_count.to_string().dimmed().to_string()
            }
        );
        println!("    {}", job.description.dimmed());
        println!();
    }
}

/// Install the agent as a system service.
pub fn install_system_service() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        install_launchd_service()
    }

    #[cfg(target_os = "linux")]
    {
        install_systemd_service()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("System service installation not supported on this platform")
    }
}

/// Uninstall the system service.
pub fn uninstall_system_service() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        uninstall_launchd_service()
    }

    #[cfg(target_os = "linux")]
    {
        uninstall_systemd_service()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("System service uninstallation not supported on this platform")
    }
}

#[cfg(target_os = "macos")]
fn install_launchd_service() -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home dir"))?;
    let binary_path = std::env::current_exe()?;
    let plist_path = home.join("Library/LaunchAgents/com.openpylot.agent.plist");

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.openpylot.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>serve</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{home}/.pylot/logs/agent.log</string>
    <key>StandardErrorPath</key>
    <string>{home}/.pylot/logs/agent.error.log</string>
</dict>
</plist>"#,
        binary = binary_path.display(),
        home = home.display(),
    );

    std::fs::create_dir_all(plist_path.parent().unwrap())?;
    std::fs::write(&plist_path, plist)?;

    // Load the service
    std::process::Command::new("launchctl")
        .args(["load", &plist_path.display().to_string()])
        .output()?;

    println!(
        "{} Installed launchd service at {}",
        "✅".bright_green(),
        plist_path.display()
    );
    println!(
        "  {} The agent will start automatically on login",
        "ℹ".bright_blue()
    );

    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_launchd_service() -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home dir"))?;
    let plist_path = home.join("Library/LaunchAgents/com.openpylot.agent.plist");

    if plist_path.exists() {
        std::process::Command::new("launchctl")
            .args(["unload", &plist_path.display().to_string()])
            .output()?;
        std::fs::remove_file(&plist_path)?;
        println!("{} Uninstalled launchd service", "✅".bright_green());
    } else {
        println!("{} No launchd service found", "ℹ".bright_blue());
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn install_systemd_service() -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home dir"))?;
    let binary_path = std::env::current_exe()?;
    let service_dir = home.join(".config/systemd/user");
    let service_path = service_dir.join("pylot.service");

    let service = format!(
        r#"[Unit]
Description=OpenPylot Personal Assistant
After=network.target

[Service]
ExecStart={binary} serve --foreground
Restart=on-failure
RestartSec=5
Environment=HOME={home}

[Install]
WantedBy=default.target
"#,
        binary = binary_path.display(),
        home = home.display(),
    );

    std::fs::create_dir_all(&service_dir)?;
    std::fs::write(&service_path, service)?;

    std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output()?;
    std::process::Command::new("systemctl")
        .args(["--user", "enable", "pylot"])
        .output()?;
    std::process::Command::new("systemctl")
        .args(["--user", "start", "pylot"])
        .output()?;

    println!(
        "{} Installed systemd service at {}",
        "✅".bright_green(),
        service_path.display()
    );

    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_systemd_service() -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot determine home dir"))?;
    let service_path = home.join(".config/systemd/user/pylot.service");

    if service_path.exists() {
        std::process::Command::new("systemctl")
            .args(["--user", "stop", "pylot"])
            .output()?;
        std::process::Command::new("systemctl")
            .args(["--user", "disable", "pylot"])
            .output()?;
        std::fs::remove_file(&service_path)?;
        std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output()?;
        println!("{} Uninstalled systemd service", "✅".bright_green());
    } else {
        println!("{} No systemd service found", "ℹ".bright_blue());
    }

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_normalize_cron() {
        // 5-field → 7-field
        assert_eq!(normalize_cron("*/5 * * * *"), "0 */5 * * * * *");
        assert_eq!(normalize_cron("0 8 * * *"), "0 0 8 * * * *");
        assert_eq!(normalize_cron("* * * * *"), "0 * * * * * *");

        // 6-field → 7-field
        assert_eq!(normalize_cron("0 */5 * * * *"), "0 0 */5 * * * *");

        // 7-field stays the same
        assert_eq!(
            normalize_cron("0 0 8 * * * *"),
            "0 0 8 * * * *"
        );
    }

    #[test]
    fn test_scheduler_state_persistence() {
        let tmp = TempDir::new().unwrap();
        let state_path = tmp.path().join("scheduler_state.json");

        let state = SchedulerState {
            jobs: vec![JobState {
                name: "test_job".to_string(),
                last_run: Some(Utc::now()),
                last_result: Some("OK".to_string()),
                run_count: 5,
                error_count: 0,
            }],
        };

        let content = serde_json::to_string_pretty(&state).unwrap();
        std::fs::write(&state_path, content).unwrap();

        let loaded = AgentScheduler::load_state(&state_path).unwrap();
        assert_eq!(loaded.jobs.len(), 1);
        assert_eq!(loaded.jobs[0].name, "test_job");
        assert_eq!(loaded.jobs[0].run_count, 5);
    }

    #[test]
    fn test_scheduler_add_job() {
        let tmp = TempDir::new().unwrap();
        let mut scheduler = AgentScheduler::new(tmp.path());

        let result = scheduler.add_job(
            "test_job",
            "A test job",
            "*/5 * * * *",
            true,
            || Box::pin(async { Ok("done".to_string()) }),
        );

        assert!(result.is_ok());
        assert_eq!(scheduler.jobs.len(), 1);
        assert_eq!(scheduler.jobs[0].name, "test_job");
        assert!(scheduler.jobs[0].enabled);
    }

    #[test]
    fn test_scheduler_list_jobs() {
        let tmp = TempDir::new().unwrap();
        let mut scheduler = AgentScheduler::new(tmp.path());

        scheduler
            .add_job(
                "job_a",
                "First job",
                "*/5 * * * *",
                true,
                || Box::pin(async { Ok("ok".to_string()) }),
            )
            .unwrap();

        scheduler
            .add_job(
                "job_b",
                "Second job",
                "0 8 * * *",
                false,
                || Box::pin(async { Ok("ok".to_string()) }),
            )
            .unwrap();

        let jobs = scheduler.list_jobs();
        assert_eq!(jobs.len(), 2);
        assert!(jobs[0].enabled);
        assert!(!jobs[1].enabled);
    }

    #[test]
    fn test_scheduler_enable_disable() {
        let tmp = TempDir::new().unwrap();
        let mut scheduler = AgentScheduler::new(tmp.path());

        scheduler
            .add_job(
                "my_job",
                "A job",
                "* * * * *",
                true,
                || Box::pin(async { Ok("ok".to_string()) }),
            )
            .unwrap();

        assert!(scheduler.jobs[0].enabled);
        scheduler.set_enabled("my_job", false);
        assert!(!scheduler.jobs[0].enabled);
        scheduler.set_enabled("my_job", true);
        assert!(scheduler.jobs[0].enabled);
    }

    #[tokio::test]
    async fn test_scheduler_run_job() {
        let tmp = TempDir::new().unwrap();
        let mut scheduler = AgentScheduler::new(tmp.path());

        scheduler
            .add_job(
                "manual_job",
                "Run manually",
                "0 0 1 1 *",
                true,
                || Box::pin(async { Ok("manual run complete".to_string()) }),
            )
            .unwrap();

        let result = scheduler.run_job("manual_job").await.unwrap();
        assert_eq!(result, "manual run complete");

        let state = scheduler
            .state
            .jobs
            .iter()
            .find(|s| s.name == "manual_job")
            .unwrap();
        assert_eq!(state.run_count, 1);
        assert!(state.last_run.is_some());
    }
}
