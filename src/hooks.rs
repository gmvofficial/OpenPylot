use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Hook configuration: pre/post-tool shell commands.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HookConfig {
    /// Command run before each tool execution. Exit 2 = deny the tool call.
    pub pre_tool_use: Option<String>,
    /// Command run after each tool execution.
    pub post_tool_use: Option<String>,
}

/// Payload sent to hook commands via stdin as JSON.
#[derive(Debug, Serialize)]
pub struct HookPayload {
    pub event: String,
    pub tool_name: String,
    pub tool_arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
}

/// Outcome of running a pre-tool hook.
#[derive(Debug)]
pub enum PreHookOutcome {
    /// Hook allowed the tool call (exit 0 or 1).
    Allow,
    /// Hook denied the tool call (exit 2).
    Deny(String),
}

/// Runs hook commands and determines outcomes.
pub struct HookRunner {
    config: HookConfig,
}

impl HookRunner {
    pub fn new(config: HookConfig) -> Self {
        Self { config }
    }

    /// Run the pre_tool_use hook. Returns Allow or Deny.
    pub async fn run_pre_tool(
        &self,
        tool_name: &str,
        tool_arguments: &serde_json::Value,
    ) -> Result<PreHookOutcome> {
        let cmd = match &self.config.pre_tool_use {
            Some(c) => c,
            None => return Ok(PreHookOutcome::Allow),
        };

        let payload = HookPayload {
            event: "pre_tool_use".to_string(),
            tool_name: tool_name.to_string(),
            tool_arguments: tool_arguments.clone(),
            tool_result: None,
            tool_success: None,
        };

        let (exit_code, output) = self.run_command(cmd, &payload).await?;

        if exit_code == 2 {
            let reason = if output.is_empty() {
                format!("Pre-tool hook denied tool '{}'", tool_name)
            } else {
                output
            };
            Ok(PreHookOutcome::Deny(reason))
        } else {
            Ok(PreHookOutcome::Allow)
        }
    }

    /// Run the post_tool_use hook (fire-and-forget, result is logged).
    pub async fn run_post_tool(
        &self,
        tool_name: &str,
        tool_arguments: &serde_json::Value,
        tool_result: &str,
        tool_success: bool,
    ) -> Result<()> {
        let cmd = match &self.config.post_tool_use {
            Some(c) => c,
            None => return Ok(()),
        };

        let payload = HookPayload {
            event: "post_tool_use".to_string(),
            tool_name: tool_name.to_string(),
            tool_arguments: tool_arguments.clone(),
            tool_result: Some(tool_result.to_string()),
            tool_success: Some(tool_success),
        };

        match self.run_command(cmd, &payload).await {
            Ok((code, output)) => {
                tracing::debug!("Post-tool hook exited {} ({})", code, output.chars().take(100).collect::<String>());
            }
            Err(e) => {
                tracing::warn!("Post-tool hook failed: {}", e);
            }
        }

        Ok(())
    }

    /// Execute a shell command with JSON payload on stdin.
    async fn run_command(&self, cmd: &str, payload: &HookPayload) -> Result<(i32, String)> {
        let json = serde_json::to_string(payload)?;

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(json.as_bytes()).await?;
            // Drop stdin to close it
        }

        let output = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait_with_output())
            .await
            .map_err(|_| anyhow::anyhow!("Hook command timed out after 10s"))??;

        let exit_code = output.status.code().unwrap_or(1);
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

        let combined = if stderr.is_empty() {
            stdout
        } else {
            format!("{}\n{}", stdout, stderr)
        };

        Ok((exit_code, combined))
    }

    /// Check if any hooks are configured.
    pub fn has_hooks(&self) -> bool {
        self.config.pre_tool_use.is_some() || self.config.post_tool_use.is_some()
    }
}
