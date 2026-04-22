use crate::mcp::types::*;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Trait for MCP transports.
#[async_trait::async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String>;
    async fn close(&self) -> Result<(), String>;
}

/// Stdio transport: communicates with an MCP server via stdin/stdout of a child process.
pub struct StdioTransport {
    child_stdin: Mutex<tokio::process::ChildStdin>,
    child_stdout: Mutex<BufReader<tokio::process::ChildStdout>>,
    _child: Mutex<Child>,
}

impl StdioTransport {
    pub async fn new(
        command: &str,
        args: &[String],
        env: Option<&HashMap<String, String>>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn MCP server: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to get stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to get stdout".to_string())?;

        Ok(Self {
            child_stdin: Mutex::new(stdin),
            child_stdout: Mutex::new(BufReader::new(stdout)),
            _child: Mutex::new(child),
        })
    }
}

#[async_trait::async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
        let mut payload = serde_json::to_string(request)
            .map_err(|e| format!("Failed to serialize request: {e}"))?;
        payload.push('\n');

        let mut stdin = self.child_stdin.lock().await;
        stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| format!("Failed to write to MCP server: {e}"))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Failed to flush stdin: {e}"))?;

        let mut stdout = self.child_stdout.lock().await;
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .await
            .map_err(|e| format!("Failed to read from MCP server: {e}"))?;

        serde_json::from_str(&line)
            .map_err(|e| format!("Failed to parse MCP response: {e}"))
    }

    async fn close(&self) -> Result<(), String> {
        let mut child = self._child.lock().await;
        let _ = child.kill().await;
        Ok(())
    }
}

/// HTTP transport: communicates with an MCP server via HTTP POST.
pub struct HttpTransport {
    url: String,
    client: reqwest::Client,
    headers: HashMap<String, String>,
}

impl HttpTransport {
    pub fn new(url: &str, headers: Option<&HashMap<String, String>>) -> Self {
        Self {
            url: url.to_string(),
            client: reqwest::Client::new(),
            headers: headers.cloned().unwrap_or_default(),
        }
    }
}

#[async_trait::async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, String> {
        let mut req = self.client.post(&self.url);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        req = req.header("Content-Type", "application/json");

        let resp = req
            .json(request)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("MCP server returned status {}", resp.status()));
        }

        resp.json::<JsonRpcResponse>()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))
    }

    async fn close(&self) -> Result<(), String> {
        Ok(())
    }
}
