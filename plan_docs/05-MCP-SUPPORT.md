# 05 — MCP (Model Context Protocol) Support

## Objective

Add MCP client support so OpenPylot can connect to any MCP server (stdio, HTTP, SSE transports), discover tools dynamically, and execute them. This opens access to 100+ community MCP servers (GitHub, Notion, Postgres, Slack, etc.) without building each integration natively.

---

## Current State

- **Tool system**: `src/tools/` — Static registry of Rust-implemented tools
- **No MCP**: No protocol support, no dynamic tool discovery, no external server connections

---

## Reference Implementations

### IronClaw (Primary — Full MCP)
- **Path**: `extra_repos/ironclaw-staging/src/tools/mcp/`
- **Features**: HTTP client transport, OAuth refresh, session manager, 202 Accepted + Streamable HTTP, extended tool definitions, resource discovery
- **CLI**: `ironclaw mcp install/list/configure`

### Claw Code (Secondary — MCP integration)
- **Path**: `extra_repos/claw-code-main/rust/`
- **Transports**: stdio, SSE, HTTP, WebSocket, SDKs, managed proxies
- **Naming**: `mcp__<servername>__<toolname>` (prevents conflicts)
- **OAuth**: MCP server auth support

---

## Architecture

### MCP Protocol Overview

MCP uses JSON-RPC 2.0 over various transports:

```
OpenPylot (Client) ←→ MCP Server (Provider)

Initialize:
  Client → { method: "initialize", params: { capabilities: {...} } }
  Server → { result: { capabilities: {...}, serverInfo: {...} } }

Discover tools:
  Client → { method: "tools/list" }
  Server → { result: { tools: [{ name, description, inputSchema }] } }

Call tool:
  Client → { method: "tools/call", params: { name: "...", arguments: {...} } }
  Server → { result: { content: [{ type: "text", text: "..." }] } }
```

### Module Structure

```
src/mcp/
├── mod.rs              -- Public API, McpManager
├── types.rs            -- JSON-RPC types, MCP messages
├── transport/
│   ├── mod.rs          -- Transport trait
│   ├── stdio.rs        -- stdio transport (local process)
│   └── http.rs         -- HTTP/SSE transport (remote server)
├── client.rs           -- MCP client: connect, discover, call
├── registry.rs         -- MCP tool → OpenPylot tool adapter
└── config.rs           -- MCP server configuration
```

---

## Implementation Steps

### Step 1: Define MCP types (Day 1 morning)

**File**: `src/mcp/types.rs`

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

// JSON-RPC 2.0
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,  // "2.0"
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

// MCP Tool Definition (from tools/list)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,       // JSON Schema
}

// MCP Tool Result (from tools/call)
#[derive(Debug, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: Value },
}

// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub transport: McpTransportType,
    pub command: Option<String>,         // For stdio: command to run
    pub args: Option<Vec<String>>,       // For stdio: command args
    pub url: Option<String>,             // For HTTP/SSE: server URL
    pub env: Option<HashMap<String, String>>,  // Environment variables
    pub headers: Option<HashMap<String, String>>, // HTTP headers
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum McpTransportType {
    #[serde(rename = "stdio")]
    Stdio,
    #[serde(rename = "http")]
    Http,
    #[serde(rename = "sse")]
    Sse,
}
```

### Step 2: Implement transports (Day 1)

**File**: `src/mcp/transport/mod.rs`

```rust
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse>;
    async fn close(&self) -> Result<()>;
}
```

**File**: `src/mcp/transport/stdio.rs`

```rust
use tokio::process::{Command, Child};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct StdioTransport {
    child: Child,
    stdin: tokio::io::BufWriter<tokio::process::ChildStdin>,
    stdout: BufReader<tokio::process::ChildStdout>,
}

impl StdioTransport {
    pub async fn new(command: &str, args: &[String], env: &HashMap<String, String>) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdin = tokio::io::BufWriter::new(child.stdin.take().unwrap());
        let stdout = BufReader::new(child.stdout.take().unwrap());

        Ok(Self { child, stdin, stdout })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        // Write JSON + newline to stdin
        let json = serde_json::to_string(&request)?;
        self.stdin.write_all(json.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        // Read response line from stdout
        let mut line = String::new();
        self.stdout.read_line(&mut line).await?;
        let response: JsonRpcResponse = serde_json::from_str(&line)?;
        Ok(response)
    }

    async fn close(&self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}
```

**File**: `src/mcp/transport/http.rs`

```rust
pub struct HttpTransport {
    client: reqwest::Client,
    url: String,
    session_id: RwLock<Option<String>>,
    headers: HashMap<String, String>,
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let mut req = self.client.post(&self.url)
            .json(&request);

        // Add session header if exists
        if let Some(sid) = self.session_id.read().await.as_ref() {
            req = req.header("Mcp-Session-Id", sid);
        }

        // Add custom headers
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        let response = req.send().await?;

        // Track session ID
        if let Some(sid) = response.headers().get("Mcp-Session-Id") {
            *self.session_id.write().await = Some(sid.to_str()?.to_string());
        }

        let body: JsonRpcResponse = response.json().await?;
        Ok(body)
    }
}
```

### Step 3: Implement MCP Client (Day 1 afternoon)

**File**: `src/mcp/client.rs`

```rust
pub struct McpClient {
    transport: Box<dyn McpTransport>,
    server_name: String,
    tools: Vec<McpToolDef>,
    next_id: AtomicU64,
}

impl McpClient {
    pub async fn connect(config: &McpServerConfig) -> Result<Self> {
        let transport: Box<dyn McpTransport> = match config.transport {
            McpTransportType::Stdio => {
                Box::new(StdioTransport::new(
                    config.command.as_ref().unwrap(),
                    config.args.as_deref().unwrap_or(&[]),
                    config.env.as_ref().unwrap_or(&HashMap::new()),
                ).await?)
            }
            McpTransportType::Http | McpTransportType::Sse => {
                Box::new(HttpTransport::new(
                    config.url.as_ref().unwrap(),
                    config.headers.clone().unwrap_or_default(),
                ).await?)
            }
        };

        let mut client = Self {
            transport,
            server_name: config.name.clone(),
            tools: vec![],
            next_id: AtomicU64::new(1),
        };

        // Initialize
        client.initialize().await?;
        // Discover tools
        client.discover_tools().await?;

        Ok(client)
    }

    async fn initialize(&self) -> Result<()> {
        let resp = self.transport.send(JsonRpcRequest {
            jsonrpc: "2.0",
            id: self.next_id(),
            method: "initialize".to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "openpylot",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        }).await?;
        // Send initialized notification
        self.transport.send(JsonRpcRequest {
            jsonrpc: "2.0",
            id: self.next_id(),
            method: "notifications/initialized".to_string(),
            params: None,
        }).await?;
        Ok(())
    }

    pub async fn discover_tools(&mut self) -> Result<&[McpToolDef]> {
        let resp = self.transport.send(JsonRpcRequest {
            jsonrpc: "2.0",
            id: self.next_id(),
            method: "tools/list".to_string(),
            params: None,
        }).await?;

        if let Some(result) = resp.result {
            let tools: Vec<McpToolDef> = serde_json::from_value(result["tools"].clone())?;
            self.tools = tools;
        }
        Ok(&self.tools)
    }

    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<McpToolResult> {
        let resp = self.transport.send(JsonRpcRequest {
            jsonrpc: "2.0",
            id: self.next_id(),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": name,
                "arguments": arguments
            })),
        }).await?;

        if let Some(error) = resp.error {
            anyhow::bail!("MCP error: {} (code {})", error.message, error.code);
        }

        let result: McpToolResult = serde_json::from_value(resp.result.unwrap())?;
        Ok(result)
    }
}
```

### Step 4: Bridge MCP tools into OpenPylot tool registry (Day 2 morning)

**File**: `src/mcp/registry.rs`

```rust
/// Wraps an MCP tool as an OpenPylot Tool
pub struct McpToolAdapter {
    client: Arc<McpClient>,
    tool_def: McpToolDef,
    server_name: String,
}

impl Tool for McpToolAdapter {
    fn name(&self) -> String {
        // Namespaced: mcp__<server>__<tool>
        format!("mcp__{}_{}", self.server_name, self.tool_def.name)
    }

    fn description(&self) -> String {
        self.tool_def.description.clone().unwrap_or_default()
    }

    fn parameters(&self) -> Value {
        self.tool_def.input_schema.clone()
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let result = self.client.call_tool(&self.tool_def.name, args).await?;
        // Convert MCP content to string
        let text = result.content.iter()
            .filter_map(|c| match c {
                McpContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }
}
```

### Step 5: Implement McpManager (Day 2)

**File**: `src/mcp/mod.rs`

```rust
pub struct McpManager {
    clients: HashMap<String, Arc<McpClient>>,
    configs: Vec<McpServerConfig>,
    tool_registry: Arc<ToolRegistry>,
}

impl McpManager {
    /// Load MCP server configs from config file
    pub fn from_config(config: &AppConfig) -> Self;

    /// Connect to all configured MCP servers and register their tools
    pub async fn connect_all(&mut self) -> Result<()> {
        for config in &self.configs {
            match McpClient::connect(config).await {
                Ok(client) => {
                    let client = Arc::new(client);
                    // Register each MCP tool as an adapter in the tool registry
                    for tool_def in client.tools() {
                        let adapter = McpToolAdapter {
                            client: client.clone(),
                            tool_def: tool_def.clone(),
                            server_name: config.name.clone(),
                        };
                        self.tool_registry.register(adapter);
                    }
                    self.clients.insert(config.name.clone(), client);
                    log::info!("Connected to MCP server: {} ({} tools)", config.name, client.tools().len());
                }
                Err(e) => {
                    log::warn!("Failed to connect to MCP server {}: {}", config.name, e);
                }
            }
        }
        Ok(())
    }

    /// Install a new MCP server from config
    pub async fn install(&mut self, config: McpServerConfig) -> Result<()>;

    /// Remove an MCP server
    pub async fn remove(&mut self, name: &str) -> Result<()>;

    /// List connected servers
    pub fn list(&self) -> Vec<&McpServerConfig>;
}
```

### Step 6: Add config and CLI (Day 2)

**Config** (`config/default.toml`):
```toml
[[mcp.servers]]
name = "filesystem"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/Users/me/docs"]

[[mcp.servers]]
name = "github"
transport = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
```

**CLI commands**:
```
pylot mcp list                        # List MCP servers
pylot mcp install <name> <config>     # Install server
pylot mcp remove <name>               # Remove server
pylot mcp tools <name>                # List tools from server
pylot mcp test <name> <tool> <args>   # Test a tool call
```

---

## Cargo.toml Dependencies

```toml
# These should already be present or easily added:
tokio = { version = "1", features = ["process", "io-util"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
serde_json = "1"
```

---

## Testing

- `test_json_rpc_serialization` — Correct JSON-RPC format
- `test_stdio_transport` — Mock process communication
- `test_http_transport` — Mock HTTP server
- `test_tool_discovery` — Parse tools/list response
- `test_tool_call` — Execute tool and parse result
- `test_tool_adapter` — MCP tool works as OpenPylot tool
- `test_namespacing` — `mcp__server__tool` naming
- `test_connection_failure` — Graceful handling

---

## Acceptance Criteria

- [ ] stdio transport connects to local MCP servers
- [ ] HTTP transport connects to remote MCP servers
- [ ] Tools discovered automatically on connection
- [ ] MCP tools callable by agent via `mcp__<server>__<tool>` name
- [ ] Tool results converted to text properly
- [ ] Config file supports multiple MCP servers
- [ ] CLI: list, install, remove, test commands work
- [ ] Connection failures handled gracefully (agent still works)
- [ ] Session management for HTTP transport
