use crate::mcp::client::McpClient;
use crate::mcp::transport::{HttpTransport, StdioTransport};
use crate::mcp::types::*;
use std::collections::HashMap;

/// Registry managing multiple MCP server connections.
pub struct McpRegistry {
    clients: HashMap<String, McpClient>,
    /// All discovered tools, prefixed as mcp_<server>_<tool>.
    tool_index: HashMap<String, (String, String)>, // prefixed_name -> (server_name, original_tool_name)
}

impl McpRegistry {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            tool_index: HashMap::new(),
        }
    }

    /// Connect to an MCP server and discover its tools.
    pub async fn add_server(&mut self, config: &McpServerConfig) -> Result<usize, String> {
        if !config.enabled {
            return Ok(0);
        }

        let transport: Box<dyn crate::mcp::transport::McpTransport> = match config.transport {
            McpTransportType::Stdio => {
                let cmd = config
                    .command
                    .as_ref()
                    .ok_or("Stdio transport requires 'command'")?;
                let args = config.args.as_deref().unwrap_or(&[]);
                Box::new(StdioTransport::new(cmd, args, config.env.as_ref()).await?)
            }
            McpTransportType::Http | McpTransportType::Sse => {
                let url = config
                    .url
                    .as_ref()
                    .ok_or("HTTP/SSE transport requires 'url'")?;
                Box::new(HttpTransport::new(url, config.headers.as_ref()))
            }
        };

        let mut client = McpClient::new(&config.name, transport);
        client.initialize().await?;
        let tools = client.discover_tools().await?;
        let tool_count = tools.len();

        // Index tools with prefixed names
        for tool in &tools {
            let prefixed = format!(
                "mcp_{}_{}",
                sanitize_name(&config.name),
                sanitize_name(&tool.name)
            );
            self.tool_index
                .insert(prefixed, (config.name.clone(), tool.name.clone()));
        }

        self.clients.insert(config.name.clone(), client);
        Ok(tool_count)
    }

    /// Call a tool by its prefixed name (mcp_<server>_<tool>).
    pub async fn call_tool(
        &self,
        prefixed_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult, String> {
        let (server_name, tool_name) = self
            .tool_index
            .get(prefixed_name)
            .ok_or_else(|| format!("Unknown MCP tool: {prefixed_name}"))?;

        let client = self
            .clients
            .get(server_name)
            .ok_or_else(|| format!("MCP server not connected: {server_name}"))?;

        client.call_tool(tool_name, arguments).await
    }

    /// Get all available MCP tools with their prefixed names.
    pub fn list_tools(&self) -> Vec<(String, McpToolDef)> {
        let mut result = Vec::new();
        for (prefixed, (server_name, original_name)) in &self.tool_index {
            if let Some(client) = self.clients.get(server_name) {
                if let Some(tool) = client.tools().iter().find(|t| &t.name == original_name) {
                    result.push((prefixed.clone(), tool.clone()));
                }
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    /// Get tool count.
    pub fn tool_count(&self) -> usize {
        self.tool_index.len()
    }

    /// Get server count.
    pub fn server_count(&self) -> usize {
        self.clients.len()
    }

    /// Get all connected server names.
    pub fn server_names(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    /// Connect every enabled server in `servers`, returning a per-server report.
    ///
    /// One server failing to start must not stop the others — a misconfigured
    /// or not-yet-installed MCP binary should cost you that server's tools, not
    /// the whole agent. Failures are reported, not returned as an error.
    pub async fn connect_all(&mut self, servers: &[McpServerConfig]) -> Vec<ConnectReport> {
        let mut reports = Vec::with_capacity(servers.len());
        for config in servers {
            if !config.enabled {
                reports.push(ConnectReport {
                    name: config.name.clone(),
                    outcome: ConnectOutcome::Disabled,
                });
                continue;
            }
            let outcome = match self.add_server(config).await {
                Ok(count) => {
                    tracing::info!("MCP server '{}' connected ({count} tools)", config.name);
                    ConnectOutcome::Connected { tools: count }
                }
                Err(e) => {
                    tracing::warn!("MCP server '{}' failed to connect: {e}", config.name);
                    ConnectOutcome::Failed(e)
                }
            };
            reports.push(ConnectReport {
                name: config.name.clone(),
                outcome,
            });
        }
        reports
    }

    /// Close all server connections.
    pub async fn close_all(&self) {
        for client in self.clients.values() {
            let _ = client.close().await;
        }
    }
}

/// What happened when we tried to bring one configured server up.
#[derive(Debug, Clone)]
pub struct ConnectReport {
    pub name: String,
    pub outcome: ConnectOutcome,
}

#[derive(Debug, Clone)]
pub enum ConnectOutcome {
    Connected { tools: usize },
    Disabled,
    Failed(String),
}

impl ConnectReport {
    pub fn is_connected(&self) -> bool {
        matches!(self.outcome, ConnectOutcome::Connected { .. })
    }
}

/// Sanitize a name for use in tool prefixes (lowercase, replace non-alphanumeric with underscore).
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("my-server"), "my_server");
        assert_eq!(sanitize_name("MyTool"), "mytool");
        assert_eq!(sanitize_name("tool.name"), "tool_name");
    }

    #[test]
    fn test_registry_new() {
        let reg = McpRegistry::new();
        assert_eq!(reg.tool_count(), 0);
        assert_eq!(reg.server_count(), 0);
    }

    fn stdio(name: &str, command: &str, enabled: bool) -> McpServerConfig {
        McpServerConfig {
            name: name.to_string(),
            transport: McpTransportType::Stdio,
            command: Some(command.to_string()),
            args: Some(vec![]),
            url: None,
            headers: None,
            env: None,
            enabled,
        }
    }

    #[tokio::test]
    async fn a_disabled_server_is_reported_without_being_launched() {
        let mut reg = McpRegistry::new();
        let reports = reg
            .connect_all(&[stdio("off", "definitely-not-a-real-binary", false)])
            .await;

        assert_eq!(reports.len(), 1);
        assert!(matches!(reports[0].outcome, ConnectOutcome::Disabled));
        assert_eq!(reg.server_count(), 0);
    }

    #[tokio::test]
    async fn one_failing_server_does_not_abort_the_rest() {
        // The whole point of per-server reports: a missing binary costs you that
        // server's tools, not every other server's.
        let mut reg = McpRegistry::new();
        let reports = reg
            .connect_all(&[
                stdio("broken", "openpylot-no-such-mcp-binary", true),
                stdio("also-broken", "openpylot-another-missing-binary", true),
            ])
            .await;

        assert_eq!(reports.len(), 2, "every server must be reported on");
        assert!(reports.iter().all(|r| matches!(r.outcome, ConnectOutcome::Failed(_))));
        assert!(reports.iter().all(|r| !r.is_connected()));
    }
}
