use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Safe list of blocked commands — these are too dangerous to execute.
const BLOCKED_COMMANDS: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd if=/dev/zero",
    ":(){ :|:& };:",  // fork bomb
    "> /dev/sda",
    "chmod -R 777 /",
];

/// Paths that should never be written to or deleted.
const PROTECTED_PATHS: &[&str] = &[
    "/etc/", "/boot/", "/usr/", "/sbin/",
    "/dev/", "/proc/", "/sys/",
    ".docker/", ".azure/", ".config/gh/",
    ".ssh/", ".gnupg/", ".aws/",
];

/// Shell/Bash execution tool inspired by Claw Code's BashCommand.
/// Runs commands in a subprocess with timeout, safety checks, and output capture.
pub struct BashTool {
    working_dir: PathBuf,
    default_timeout_ms: u64,
}

impl BashTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            default_timeout_ms: 120_000, // 2 minutes
        }
    }

    /// Check if a command is blocked.
    fn is_blocked(command: &str) -> Option<&'static str> {
        let cmd_lower = command.to_lowercase();
        for blocked in BLOCKED_COMMANDS {
            if cmd_lower.contains(&blocked.to_lowercase()) {
                return Some(blocked);
            }
        }
        // Check for protected path writes
        for path in PROTECTED_PATHS {
            if (cmd_lower.contains("rm ") || cmd_lower.contains("chmod ") || cmd_lower.contains("chown "))
                && cmd_lower.contains(path)
            {
                return Some(path);
            }
        }
        None
    }

    /// Sanitize output to cap length.
    fn truncate_output(output: &str, max_chars: usize) -> String {
        if output.len() > max_chars {
            let half = max_chars / 2;
            let start: String = output.chars().take(half).collect();
            let end: String = output.chars().rev().take(half).collect::<String>().chars().rev().collect();
            format!(
                "{}\n\n... [truncated {} chars] ...\n\n{}",
                start,
                output.len() - max_chars,
                end
            )
        } else {
            output.to_string()
        }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a shell command in a bash subprocess. Use this for running programs, \
                installing packages, compiling code, running tests, git operations, and system commands. \
                The command runs in the project working directory. Output is captured and returned.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in milliseconds (default: 120000 = 2 minutes)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Brief description of what this command does (for logging)"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Optional working directory override (defaults to project root)"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let command = params
            .get("command")
            .and_then(|c| c.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'command' parameter"))?;

        let timeout_ms = params
            .get("timeout")
            .and_then(|t| t.as_u64())
            .unwrap_or(self.default_timeout_ms);

        let description = params
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("");

        let working_dir = params
            .get("working_dir")
            .and_then(|w| w.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.working_dir.clone());

        // Safety: block dangerous commands
        if let Some(blocked) = Self::is_blocked(command) {
            return Ok(ToolResult::err(format!(
                "Command blocked for safety: matches dangerous pattern '{}'",
                blocked
            )));
        }

        // Log the command
        if !description.is_empty() {
            tracing::info!("bash: {} — {}", description, command);
        } else {
            tracing::info!("bash: {}", command);
        }

        // Execute with timeout
        let result = timeout(
            Duration::from_millis(timeout_ms),
            async {
                let output = Command::new("sh")
                    .arg("-lc")
                    .arg(command)
                    .current_dir(&working_dir)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .stdin(Stdio::null())
                    .output()
                    .await;
                output
            }
        ).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                // Build response
                let mut response = String::new();

                if exit_code != 0 {
                    response.push_str(&format!("Exit code: {}\n", exit_code));
                }

                if !stdout.is_empty() {
                    response.push_str(&Self::truncate_output(&stdout, 50_000));
                }

                if !stderr.is_empty() {
                    if !response.is_empty() {
                        response.push_str("\n--- stderr ---\n");
                    }
                    response.push_str(&Self::truncate_output(&stderr, 10_000));
                }

                if response.is_empty() {
                    response = format!("Command completed with exit code {}", exit_code);
                }

                if output.status.success() {
                    Ok(ToolResult::ok(response))
                } else {
                    Ok(ToolResult::err(response))
                }
            }
            Ok(Err(e)) => {
                Ok(ToolResult::err(format!("Failed to execute command: {}", e)))
            }
            Err(_) => {
                Ok(ToolResult::err(format!(
                    "Command timed out after {}ms. The command may still be running in the background.",
                    timeout_ms
                )))
            }
        }
    }
}
