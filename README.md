# GMV Agent

A Rust-powered personal AI assistant with CLI, Web Dashboard, Python, and Node.js interfaces. Schedule meetings, monitor RSVPs, send and search emails, manage notes, set reminders, and interact through a real-time web chat, Telegram, or WhatsApp — all backed by an encrypted secrets vault and a pluggable LLM provider (OpenAI / Anthropic).

---

## Features

| Category | Capabilities |
|----------|-------------|
| **LLM Providers** | OpenAI (GPT-4o) and Anthropic (Claude) with hot-swappable configuration |
| **Google Calendar** | OAuth 2.0 login, list/create events, create meetings with Google Meet links |
| **Gmail** | Search emails, send & reply, create/send/delete drafts — uses the same Google OAuth credentials |
| **Web Dashboard** | Next.js web UI with real-time chat (WebSocket), integrations setup, knowledge base, settings, and persistent conversation history |
| **Telegram** | Send messages, receive updates, full bot mode with slash commands |
| **WhatsApp** | Send messages via Twilio |
| **Notes** | Create, list, search, delete — stored locally as JSON |
| **Reminders** | Set, list, complete — with scheduled background checks |
| **Scheduler** | Cron-based background jobs: RSVP monitor, meeting reminders, daily briefing, calendar sync, token refresh, email digest |
| **Webhooks** | Receive push notifications from Google Calendar, Gmail, GitHub, and Slack |
| **OAuth** | Browser-based OAuth 2.0 flows for Google, GitHub, and Slack |
| **Secrets Vault** | AES-256-GCM encrypted, machine-bound credential storage |
| **Setup Wizard** | Interactive `init` command that walks you through everything |
| **Background Service** | Install as a launchd (macOS) or systemd (Linux) service |

---

## Quick Start

### Install

Choose the method that fits your setup:

```bash
# One-line installer (macOS / Linux)
curl -fsSL https://raw.githubusercontent.com/GMV-AI/gmv-agent/main/install.sh | bash

# Homebrew
brew tap GMV-AI/tap
brew install gmv-agent

# Docker
docker compose up -d

# From source
cargo build --release && sudo cp target/release/gmv-agent /usr/local/bin/

# Python (also installs CLI shim)
pip install gmv-agent

# Node.js (also installs CLI shim)
npm install -g gmv-agent
```

### First Run

```bash
gmv-agent init          # Interactive setup wizard
gmv-agent doctor        # Verify everything is configured
gmv-agent               # Start interactive REPL
```

The `init` wizard guides you through:
1. LLM provider selection and API key
2. Agent name and persona
3. Integrations (Google Calendar & Gmail, Telegram, WhatsApp)
4. Notification preferences
5. Background scheduler configuration

All secrets are encrypted and stored in `~/.gmv-agent/secrets.enc`.

---

## Usage — Rust CLI

The native binary is the primary interface. All other bindings (Python, Node.js) delegate to it.

### Interactive Mode (REPL)

```bash
gmv-agent
```

```
You> What meetings do I have today?
🔧 Calling tool: list_calendar_events
📅 Team Standup — 10:00 AM

GMV Agent: You have one meeting today — Team Standup at 10:00 AM.

You> Take a note: Review Q1 roadmap before the all-hands
🔧 Calling tool: create_note
✅ Note created.

You> /tools
Available tools: create_note, list_notes, search_notes, delete_note,
  list_calendar_events, create_calendar_event, create_meeting,
  gmail_search, gmail_get, gmail_send, gmail_reply,
  gmail_draft_create, gmail_draft_send, gmail_draft_delete,
  set_reminder, list_reminders, complete_reminder,
  send_telegram_message, get_telegram_updates, send_whatsapp_message
```

### One-Shot Chat

```bash
gmv-agent chat "Schedule a meeting with alice@example.com tomorrow at 2pm for 1 hour about Q1 planning"
```

### REPL Commands

| Command  | Description                |
|----------|----------------------------|
| `/clear` | Clear conversation history |
| `/tools` | List loaded tools          |
| `/help`  | Show help                  |
| `/quit`  | Exit the agent             |

### CLI Reference

| Command | Description |
|---------|-------------|
| `gmv-agent` | Interactive REPL |
| `gmv-agent init` | Setup wizard (`--reset` to start fresh, `--only <service>` for one integration) |
| `gmv-agent chat "<message>"` | One-shot query |
| `gmv-agent add <service>` | Add an integration (google-calendar, telegram, whatsapp, github, slack) |
| `gmv-agent remove <service>` | Remove an integration |
| `gmv-agent doctor` | Diagnostic checks |
| `gmv-agent status` | Show agent status and connected services |
| `gmv-agent tools` | List available tools |
| `gmv-agent telegram-bot` | Start Telegram bot mode |
| `gmv-agent serve` | Start background daemon with scheduler |
| `gmv-agent serve install` | Install as system service (launchd / systemd) |
| `gmv-agent serve uninstall` | Remove system service |
| `gmv-agent jobs list` | List scheduled jobs |
| `gmv-agent jobs run <name>` | Run a job immediately |
| `gmv-agent jobs enable <name>` | Enable a job |
| `gmv-agent jobs disable <name>` | Disable a job |
| `gmv-agent config list` | Show current configuration |
| `gmv-agent config set <key> <value>` | Update a configuration value |
| `gmv-agent logs` | Tail agent logs (`--scheduler` for scheduler logs) |

---

## Usage — Python SDK

### Install

```bash
pip install gmv-agent
```

> The Rust binary must also be on your `PATH`. The Python package wraps the native binary via PyO3.

### Interactive Setup

```python
from gmv_agent import GMVAgent

GMVAgent.init()  # Launches the terminal wizard
```

### Chat

```python
from gmv_agent import GMVAgent

agent = GMVAgent.from_config("~/.gmv-agent/secrets.enc")
response = agent.chat("What meetings do I have today?")
print(response)
```

### Programmatic / CI Configuration

```python
from gmv_agent import GMVAgent, Config

config = Config(
    llm_provider="openai",
    llm_model="gpt-4o",
    openai_api_key="sk-...",
    telegram_bot_token="...",
)
agent = GMVAgent(config)
response = agent.chat("Schedule a meeting with John tomorrow at 3pm")
print(response)
```

### Custom Tools

```python
def search_web(query: str) -> str:
    """Your custom tool implementation."""
    return "results..."

agent.register_tool(
    name="search_web",
    schema='{"type":"object","properties":{"query":{"type":"string"}}}',
    callback=search_web,
)
```

### CLI

Once installed, the CLI shim delegates to the Rust binary:

```bash
gmv-agent init
gmv-agent chat "What's on my calendar?"
gmv-agent doctor
```

See [python/README.md](python/README.md) for full Python documentation.

---

## Usage — Node.js / TypeScript SDK

### Install

```bash
npm install gmv-agent
# or
yarn add gmv-agent
```

> The Rust binary must also be on your `PATH`. The Node.js package wraps the native binary via NAPI-RS.

### Interactive Setup

```typescript
import { GMVAgent } from 'gmv-agent';

await GMVAgent.init();  // Launches the terminal wizard
```

### Chat

```typescript
import { GMVAgent } from 'gmv-agent';

const agent = await GMVAgent.fromConfig('~/.gmv-agent/secrets.enc');
const response = await agent.chat('What meetings do I have today?');
console.log(response);
```

### Programmatic / CI Configuration

```typescript
import { GMVAgent, Config } from 'gmv-agent';

const config: Config = {
  llmProvider: 'anthropic',
  llmModel: 'claude-sonnet-4-20250514',
  anthropicApiKey: process.env.ANTHROPIC_API_KEY,
};

const agent = new GMVAgent(config);
const response = await agent.chat('Set a reminder for 5pm to review PRs');
console.log(response);
```

### Diagnostics

```typescript
await GMVAgent.doctor();   // Run diagnostic checks
await GMVAgent.status();   // Show connected services
```

### CLI

The npm package includes a CLI shim:

```bash
npx gmv-agent init
npx gmv-agent chat "Hello"
```

---

## Configuration

GMV Agent uses a layered configuration system (highest to lowest priority):

1. **Environment variables**
2. **Encrypted secrets vault** (`~/.gmv-agent/secrets.enc`)
3. **TOML config files** (`config/default.toml` or `~/.gmv-agent/config.toml`)
4. **Built-in defaults**

### Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `OPENAI_API_KEY` | OpenAI API key | If using OpenAI |
| `ANTHROPIC_API_KEY` | Anthropic API key | If using Anthropic |
| `LLM_PROVIDER` | `openai` or `anthropic` | No (default: `openai`) |
| `LLM_MODEL` | Model name | No (auto-detected) |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID | For Calendar |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret | For Calendar |
| `TELEGRAM_BOT_TOKEN` | Telegram bot token | For Telegram |
| `TELEGRAM_DEFAULT_CHAT_ID` | Default Telegram chat ID | For Telegram |
| `TWILIO_ACCOUNT_SID` | Twilio account SID | For WhatsApp |
| `TWILIO_AUTH_TOKEN` | Twilio auth token | For WhatsApp |
| `TWILIO_WHATSAPP_FROM` | Sender number | For WhatsApp |
| `AGENT_NAME` | Agent display name | No |
| `AGENT_PERSONA` | Agent personality | No |

### TOML Configuration

Edit `config/default.toml` or `~/.gmv-agent/config.toml`:

```toml
[agent]
name = "My Assistant"
persona = "You are a helpful personal assistant."
max_context_messages = 50
max_tool_iterations = 15

[llm]
provider = "openai"
model = "gpt-4o"
max_tokens = 4096
temperature = 0.7

[google_calendar]
enabled = true
redirect_port = 8085

[telegram]
enabled = true

[scheduler]
enabled = true
```

### Encrypted Secrets Vault

After running `gmv-agent init`, secrets are stored encrypted at `~/.gmv-agent/secrets.enc`:
- **AES-256-GCM** encryption
- **Machine-bound** — encrypted with your machine's unique ID
- **No plaintext API keys** on disk

---

## Integrations Setup

### Google Calendar

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a project and enable the **Google Calendar API**
3. Create **OAuth 2.0 credentials** (Desktop app type)
4. Add `http://localhost:8085` as an authorized redirect URI
5. Run:
   ```bash
   gmv-agent add google-calendar
   ```
   This opens your browser for consent and stores tokens locally.

### Telegram Bot

1. Message [@BotFather](https://t.me/botfather) on Telegram
2. Send `/newbot` and follow the prompts
3. Run:
   ```bash
   gmv-agent add telegram
   ```
4. Start bot mode:
   ```bash
   gmv-agent telegram-bot
   ```

Bot commands: `/start`, `/help`, `/tools`, `/clear`

### WhatsApp (via Twilio)

1. Create a [Twilio account](https://console.twilio.com/)
2. Set up a WhatsApp sandbox or apply for a business number
3. Run:
   ```bash
   gmv-agent add whatsapp
   ```

---

## Background Service & Scheduler

GMV Agent can run as a daemon with cron-based scheduled jobs.

### Scheduled Jobs

| Job | Default Schedule | Description |
|-----|-----------------|-------------|
| `reminder_check` | Every 1 min | Check and fire due reminders |
| `rsvp_monitor` | Every 15 min | Detect RSVP changes on calendar events |
| `meeting_reminder` | Every 5 min | Send upcoming meeting notifications |
| `calendar_sync` | Every 30 min | Sync calendar events |
| `token_refresh` | Every 45 min | Refresh OAuth tokens |
| `daily_briefing` | 8:00 AM | Morning summary of the day |
| `email_digest` | 6:00 PM | Evening email digest |

### Install as System Service

```bash
# macOS (launchd — creates ~/Library/LaunchAgents/com.gmv.agent.plist)
gmv-agent serve install

# Linux (systemd — creates ~/.config/systemd/user/gmv-agent.service)
gmv-agent serve install
```

### Manage

```bash
gmv-agent status            # Check status
gmv-agent logs              # Tail agent logs
gmv-agent jobs list         # List scheduled jobs
gmv-agent jobs run <name>   # Run a job now
gmv-agent serve uninstall   # Remove service
```

---

## Web Dashboard

GMV Agent ships with a Next.js web UI that lets you manage everything from the browser.

### Starting the Web UI

```bash
# 1. Start the backend API server (port 3001)
gmv-agent serve

# 2. In another terminal, start the frontend dev server (port 3000)
cd frontend
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) to access the dashboard.

### Building for Production

```bash
cd frontend
npm run build    # Generates static files in frontend/out/
```

The static build can be served by the Rust backend directly or any static file server.

### Pages

| Page | Path | Description |
|------|------|-------------|
| **Home** | `/` | Landing page with quick links |
| **Chat** | `/chat` | Real-time chat with the agent via WebSocket |
| **Integrations** | `/setup` | Connect & disconnect services (Google, Telegram, WhatsApp, etc.) |
| **Knowledge Base** | `/knowledge` | Manage document collections, upload text, and search |
| **Dashboard** | `/dashboard` | Agent status, scheduled jobs, recent logs |
| **Settings** | `/settings` | Agent config (name, persona, model, temperature) and memory management |

### Connecting Integrations via the Web UI

1. Navigate to **Integrations** (`/setup`)
2. Click **Connect** on a service
3. For **Telegram / WhatsApp / GitHub / Slack**: a credential modal appears — enter your tokens or API keys
4. For **Google Calendar / Gmail**: the backend starts an OAuth flow and opens the authorization page in a new tab
5. After completing the flow, the integration shows as **Connected** (the page polls for status changes)
6. Use the **Test** button to verify connectivity
7. Use **Disconnect** to remove stored credentials

Integrations configured via the CLI (`gmv-agent init` or `gmv-agent add`) are automatically visible in the web UI.

### Knowledge Base

1. Navigate to **Knowledge Base** (`/knowledge`)
2. Create a collection, then click into it
3. Use **Upload** to add a text document (paste content or import a `.txt` / `.md` file)
4. The **Search** bar performs keyword search across all documents

### API Endpoints

The backend API (default `http://localhost:3001`) exposes:

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/api/status` | Agent status |
| `GET` | `/api/integrations` | List integrations with live vault status |
| `POST` | `/api/integrations/{service}/connect` | Connect (accepts optional `credentials` body) |
| `DELETE` | `/api/integrations/{service}` | Disconnect |
| `POST` | `/api/integrations/{service}/test` | Test connectivity |
| `GET` | `/api/knowledge/collections` | List collections |
| `POST` | `/api/knowledge/collections` | Create collection |
| `GET` | `/api/knowledge/documents` | List all documents |
| `POST` | `/api/knowledge/documents` | Upload document |
| `POST` | `/api/knowledge/search` | Search documents |
| `GET` | `/api/jobs` | List scheduled jobs |
| `PATCH` | `/api/jobs/{name}` | Update job (enable/disable) |
| `POST` | `/api/jobs/{name}/run` | Run job immediately |
| `GET` | `/api/settings` | Get agent settings |
| `PATCH` | `/api/settings` | Update settings |
| `GET` | `/api/memory` | List memory facts |
| `GET` | `/api/logs` | Recent logs (supports `?level=` and `?limit=` query params) |
| `WS` | `/ws/chat` | WebSocket for real-time chat |
| `WS` | `/ws/notifications` | WebSocket for push notifications |

---

## Docker

```bash
# Build and run with docker compose
docker compose up -d

# Or build manually
docker build -t gmv-agent .
docker run --rm -it \
  -v ~/.gmv-agent:/home/gmv/.gmv-agent \
  -e OPENAI_API_KEY=sk-... \
  -p 3001:3001 \
  -p 8443:8443 \
  gmv-agent
```

The `docker-compose.yml` includes volume mounts for `~/.gmv-agent` to persist configuration and data.

---

## Project Structure

```
├── src/
│   ├── main.rs            # CLI entry point (clap), all command handlers
│   ├── agent.rs           # Agent loop: LLM <-> tool calls
│   ├── config.rs          # Layered configuration (env > vault > TOML > defaults)
│   ├── context.rs         # Conversation context management
│   ├── memory.rs          # Persistent memory store
│   ├── terminal.rs        # Interactive REPL
│   ├── secrets.rs         # AES-256-GCM encrypted vault
│   ├── init.rs            # Setup wizard, doctor, status
│   ├── scheduler.rs       # Tokio cron scheduler (7 jobs)
│   ├── oauth.rs           # Browser-based OAuth 2.0 flows
│   ├── telegram_bot.rs    # Telegram long-polling bot
│   ├── lib.rs             # Library crate (re-exports modules)
│   ├── api/
│   │   ├── mod.rs         # Axum router & embedded static file server
│   │   ├── handlers.rs    # REST API handlers (integrations, knowledge, settings)
│   │   └── ws.rs          # WebSocket chat & notification handlers
│   ├── llm/
│   │   ├── mod.rs         # LLM provider trait & types
│   │   ├── openai.rs      # OpenAI provider
│   │   └── anthropic.rs   # Anthropic (Claude) provider
│   ├── tools/
│   │   ├── mod.rs         # Tool trait, ToolRegistry
│   │   ├── notes.rs       # Note CRUD
│   │   ├── calendar.rs    # Google Calendar (OAuth2 + API)
│   │   ├── gmail.rs       # Gmail integration
│   │   ├── telegram.rs    # Telegram Bot API
│   │   ├── whatsapp.rs    # WhatsApp via Twilio
│   │   └── reminder.rs    # Reminder management
│   ├── webhooks/
│   │   ├── mod.rs
│   │   └── server.rs      # Webhook endpoints (Calendar, Gmail, GitHub, Slack)
│   └── jobs/
│       ├── mod.rs         # Job trait & registry
│       ├── rsvp_monitor.rs# RSVP change detection
│       └── reminders.rs   # Meeting reminder notifications
├── frontend/              # Next.js 15 web dashboard
│   ├── src/
│   │   ├── app/           # App Router pages (chat, setup, knowledge, dashboard, settings)
│   │   ├── components/    # UI components (chat bubbles, layout, shadcn/ui)
│   │   ├── lib/           # API client, utility helpers, WebSocket manager
│   │   ├── stores/        # Zustand stores (app, chat, notifications, toast)
│   │   └── types/         # TypeScript type definitions
│   ├── package.json
│   └── tailwind.config.ts
├── python/                # Python bindings (PyO3 + maturin)
│   ├── src/lib.rs
│   ├── python/gmv_agent/
│   ├── pyproject.toml
│   └── README.md
├── node/                  # Node.js/TS bindings (NAPI-RS)
│   ├── src/lib.rs
│   ├── js/index.ts
│   ├── js/cli.js
│   ├── package.json
│   └── Cargo.toml
├── config/
│   └── default.toml       # Default TOML configuration
├── docs/
│   ├── INSTALLATION.md    # Full installation guide
│   ├── project_requirement.md
│   └── setup_and_integration_guide.md
├── tests/                 # Test suites (Rust, Python, Node.js)
├── .github/workflows/
│   └── ci.yml             # CI/CD: lint, test, cross-compile, publish
├── Dockerfile
├── docker-compose.yml
├── Formula/gmv-agent.rb   # Homebrew formula
├── install.sh             # One-line installer script
├── CONTRIBUTING.md
└── Cargo.toml
```

---

## Data Storage

All runtime data is stored locally at `~/.gmv-agent/`:

```
~/.gmv-agent/
├── bin/gmv-agent          # Binary (if installed via installer)
├── secrets.enc            # Encrypted secrets vault
├── config.toml            # User configuration overrides
└── data/
    ├── notes.json                  # Saved notes
    ├── reminders.json              # Saved reminders
    ├── memory.json                 # Agent memory (facts & summaries)
    ├── google_tokens.json          # Google Calendar OAuth2 tokens
    ├── gmail_tokens.json           # Gmail OAuth2 tokens
    ├── knowledge_collections.json  # Knowledge base collections
    ├── knowledge_documents.json    # Knowledge base documents
    ├── history.txt                 # REPL command history
    └── conversations/              # Persistent chat conversations (one JSON per conversation)
```

---

## Development

### Prerequisites

- **Rust 1.75+** — [rustup.rs](https://rustup.rs/)
- **Python 3.9+** and **maturin** — for Python bindings
- **Node.js 18+** and **@napi-rs/cli** — for Node.js bindings

### Build & Test

```bash
# Build Rust
cargo build --release
cargo test

# Python bindings
cd python
pip install maturin
maturin develop
pytest

# Node.js bindings
cd node
npm install
npm run build
npm test
```

### Adding a New Tool

1. Create `src/tools/your_tool.rs`
2. Implement the `Tool` trait:

```rust
use async_trait::async_trait;
use serde_json::{json, Value};
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct YourTool;

#[async_trait]
impl Tool for YourTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "your_tool_action".into(),
            description: "What this tool does".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string", "description": "..." }
                },
                "required": ["input"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> anyhow::Result<ToolResult> {
        let input = params["input"].as_str().unwrap_or_default();
        // ... implementation ...
        Ok(ToolResult::ok("Done!"))
    }
}
```

3. Register in `src/main.rs`:
```rust
tools.register(Box::new(your_tool::YourTool::new()));
```

### Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, commit conventions, and PR guidelines.

---

## Troubleshooting

| Problem | Solution |
|---------|----------|
| "No LLM API key configured" | Run `gmv-agent init` or set `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` |
| "Secrets file is corrupted" | Back up `~/.gmv-agent/secrets.enc` and re-run `gmv-agent init` |
| Google OAuth fails | Ensure port 8085 is free (`lsof -i :8085`), or set `GOOGLE_REDIRECT_PORT` |
| Binary not found (Python/Node) | The Rust binary must be on your `PATH` — install it first |
| Debug logging | `RUST_LOG=debug gmv-agent` |

Run `gmv-agent doctor` to diagnose issues automatically.

---

## License

MIT
