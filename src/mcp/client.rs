use crate::mcp::transport::McpTransport;
use crate::mcp::types::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// MCP client: manages connection to a single MCP server.
pub struct McpClient {
    pub server_name: String,
    transport: Box<dyn McpTransport>,
    request_id: AtomicU64,
    tools: Vec<McpToolDef>,
    initialized: bool,
}

impl McpClient {
    pub fn new(server_name: &str, transport: Box<dyn McpTransport>) -> Self {
        Self {
            server_name: server_name.to_string(),
            transport,
            request_id: AtomicU64::new(1),
            tools: Vec::new(),
            initialized: false,
        }
    }

    fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Initialize the MCP session with the server.
    pub async fn initialize(&mut self) -> Result<(), String> {
        let req = JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "pylot",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let resp = self.transport.send(&req).await?;
        if let Some(err) = resp.error {
            return Err(format!("MCP initialize error: {}", err.message));
        }

        // Send initialized notification
        let notif = JsonRpcRequest::new(self.next_id(), "notifications/initialized", None);
        let _ = self.transport.send(&notif).await;

        self.initialized = true;
        Ok(())
    }

    /// Discover tools from the MCP server.
    pub async fn discover_tools(&mut self) -> Result<Vec<McpToolDef>, String> {
        if !self.initialized {
            return Err("MCP client not initialized".to_string());
        }

        let req = JsonRpcRequest::new(self.next_id(), "tools/list", None);
        let resp = self.transport.send(&req).await?;

        if let Some(err) = resp.error {
            return Err(format!("MCP tools/list error: {}", err.message));
        }

        let result = resp.result.ok_or("No result in tools/list response")?;
        let tools_value = result
            .get("tools")
            .ok_or("No 'tools' field in response")?;

        #[derive(serde::Deserialize)]
        struct RawTool {
            name: String,
            #[serde(default)]
            description: String,
            #[serde(default = "default_schema")]
            #[serde(rename = "inputSchema")]
            input_schema: serde_json::Value,
        }

        fn default_schema() -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {}})
        }

        let raw_tools: Vec<RawTool> = serde_json::from_value(tools_value.clone())
            .map_err(|e| format!("Failed to parse tools: {e}"))?;

        self.tools = raw_tools
            .into_iter()
            .map(|t| McpToolDef {
                name: t.name,
                description: t.description,
                input_schema: t.input_schema,
                server_name: self.server_name.clone(),
            })
            .collect();

        Ok(self.tools.clone())
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult, String> {
        if !self.initialized {
            return Err("MCP client not initialized".to_string());
        }

        let req = JsonRpcRequest::new(
            self.next_id(),
            "tools/call",
            Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments
            })),
        );

        let resp = self.transport.send(&req).await?;

        if let Some(err) = resp.error {
            return Err(format!("MCP tool call error: {}", err.message));
        }

        let result = resp.result.ok_or("No result in tool call response")?;

        // Parse the MCP content response
        let content = result
            .get("content")
            .and_then(|c| serde_json::from_value::<Vec<McpContent>>(c.clone()).ok())
            .unwrap_or_default();

        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(McpToolResult { content, is_error })
    }

    /// Get the list of discovered tools.
    pub fn tools(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// Close the connection.
    pub async fn close(&self) -> Result<(), String> {
        self.transport.close().await
    }
}
