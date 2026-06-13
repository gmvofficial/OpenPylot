<p align="center">
  <strong>OpenPylot</strong><br>
  <em>A Rust-powered personal AI assistant</em>
</p>

<p align="center">
  <a href="https://github.com/openpylot/pylot"><img src="https://img.shields.io/badge/version-0.3.0-blue" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="License"></a>
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.75%2B-orange" alt="Rust"></a>
  <a href="https://github.com/openpylot/pylot/actions"><img src="https://img.shields.io/badge/tests-100%20passing-brightgreen" alt="Tests"></a>
</p>

---

OpenPylot is a modular, extensible personal AI assistant built in Rust. It ships as a single binary with a CLI, Web Dashboard, Python SDK, and Node.js SDK. Connect your calendar, email, social media, messaging apps, and documents — all backed by an encrypted secrets vault, pluggable LLM providers, a smart memory system, and autonomous sub-agents.

## Table of Contents

- [Features](#features)
- [What's New in v0.3.0](#whats-new-in-v030)
- [Quick Start](#quick-start)
- [Usage — CLI](#usage--cli)
- [Usage — Python SDK](#usage--python-sdk)
- [Usage — Node.js SDK](#usage--nodejs-sdk)
- [Configuration](#configuration)
- [Integrations](#integrations)
- [Advanced Features](#advanced-features)
- [Web Dashboard](#web-dashboard)
- [Background Service & Scheduler](#background-service--scheduler)
- [Docker](#docker)
- [API Reference](#api-reference)
- [Project Structure](#project-structure)
- [Development](#development)
- [Troubleshooting](#troubleshooting)
- [Documentation](#documentation)
- [License](#license)

---

## Features

| Category              | Capabilities                                                                                                                                                                          |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **LLM Providers**     | OpenAI (GPT-4o) and Anthropic (Claude) with hot-swappable configuration                                                                                                               |
| **Smart Memory**      | SQLite-backed semantic memory with OpenAI embeddings — personal facts, knowledge base, auto-extraction                                                                                |
| **Skills System**     | Declarative SKILL.md skills with YAML frontmatter — pattern-matched to user intents at runtime                                                                                        |
| **Sub-Agents**        | Spawn specialist sub-agents (researcher, coder, marketing) with isolated context and tool access                                                                                      |
| **Streaming**         | Real-time token streaming over WebSocket and SSE for responsive chat experiences                                                                                                      |
| **MCP Support**       | Model Context Protocol — connect external tool servers via JSON-RPC                                                                                                                   |
| **Learning**          | LLM-as-judge auto-scoring, prompt evolution, and automatic skill generation from failure patterns                                                                                     |
| **Social Media**      | 17 platform providers — Twitter/X, LinkedIn, Bluesky, Facebook, Instagram, TikTok, YouTube, Pinterest, Reddit, Threads, Mastodon, Discord, Slack, Medium, Dev.to, Hashnode, WordPress |
| **Marketing Agent**   | Campaign planning, content strategies, content generation with approval workflow                                                                                                      |
| **Google Calendar**   | OAuth 2.0 login, list/create events, create meetings with Google Meet links                                                                                                           |
| **Gmail**             | Search emails, send & reply, create/send/delete drafts                                                                                                                                |
| **Telegram**          | Full bot mode with slash commands, send/receive messages                                                                                                                              |
| **WhatsApp**          | Send messages via Twilio                                                                                                                                                              |
| **Web Dashboard**     | Next.js web UI with real-time chat (WebSocket), integrations, knowledge base, settings                                                                                                |
| **Notes & Reminders** | Create, list, search, delete — stored locally with scheduled background checks                                                                                                        |
| **Scheduler**         | Cron-based background jobs: RSVP monitor, meeting reminders, daily briefing, email digest                                                                                             |
| **Webhooks**          | Receive push notifications from Google Calendar, Gmail, GitHub, and Slack                                                                                                             |
| **Secrets Vault**     | AES-256-GCM encrypted, machine-bound credential storage with Argon2id KDF                                                                                                             |
| **Python SDK**        | PyO3 bindings — `pip install pylot`                                                                                                                                                   |
| **Node.js SDK**       | NAPI-RS bindings — `npm install pylot`                                                                                                                                                |

---

## What's New in v0.3.0

- **17 social media platforms** — Facebook, Instagram, TikTok, YouTube, Pinterest, Reddit, Threads, Mastodon, Discord, Slack, Medium, Dev.to, Hashnode, WordPress (plus existing Twitter, LinkedIn, Bluesky)
- **Smart Memory v2** — SQLite-backed semantic search with OpenAI embeddings, auto-extraction of personal facts
- **Skills System** — Declarative skills loaded from SKILL.md files, matched to user intents via embedding similarity
- **Sub-Agent System** — Spawn specialist agents with isolated context, delegated tool access, and configurable models
- **MCP Support** — Model Context Protocol integration for connecting external tool servers
- **Learning Engine** — LLM-as-judge auto-scoring (majority vote), prompt evolution, automatic SKILL.md generation from failure patterns
- **Streaming Responses** — Token-by-token streaming over WebSocket and SSE
- **Marketing Agent** — Campaign planning, content strategy generation, approval workflow
- **Full config wiring** — All platforms, MCP, learning, and marketing fully configurable via env / vault / TOML

---

## Quick Start

### Install

```bash
# One-line installer (macOS / Linux)
curl -fsSL https://raw.githubusercontent.com/openpylot/pylot/main/install.sh | bash

# Homebrew
brew tap openpylot/tap
brew install pylot

# Docker
docker compose up -d

# From source
cargo build --release && sudo cp target/release/pylot /usr/local/bin/

# Python
pip install pylot

# Node.js
npm install -g pylot
```

### First Run

```bash
pylot init          # Interactive setup wizard
pylot doctor        # Verify everything is configured
pylot               # Start interactive REPL
```

The `init` wizard guides you through:

1. LLM provider selection and API key
2. Agent name and persona
3. Integrations (Google Calendar & Gmail, Telegram, WhatsApp)
4. Notification preferences
5. Background scheduler configuration

All secrets are encrypted and stored in `~/.pylot/secrets.enc`.

---

## Usage — CLI

The native binary is the primary interface.

### Interactive Mode (REPL)

```bash
pylot
```

```
You> What meetings do I have today?
🔧 Calling tool: list_calendar_events
📅 Team Standup — 10:00 AM

Pylot: You have one meeting today — Team Standup at 10:00 AM.

You> Take a note: Review Q1 roadmap before the all-hands
🔧 Calling tool: create_note
✅ Note created.

You> /tools
Available tools: create_note, list_notes, search_notes, delete_note,
  list_calendar_events, create_calendar_event, create_meeting,
  gmail_search, gmail_get, gmail_send, gmail_reply,
  gmail_draft_create, gmail_draft_send, gmail_draft_delete,
  set_reminder, list_reminders, complete_reminder,
  send_telegram_message, get_telegram_updates, send_whatsapp_message,
  memory_store, memory_search, memory_list
```

### One-Shot Chat

```bash
pylot chat "Schedule a meeting with alice@example.com tomorrow at 2pm"
```

### REPL Commands

| Command  | Description                |
| -------- | -------------------------- |
| `/clear` | Clear conversation history |
| `/tools` | List loaded tools          |
| `/help`  | Show help                  |
| `/quit`  | Exit the agent             |

### CLI Reference

| Command                          | Description                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------- |
| `pylot`                          | Interactive REPL                                                                |
| `pylot init`                     | Setup wizard (`--reset` to start fresh, `--only <service>` for one integration) |
| `pylot chat "<message>"`         | One-shot query                                                                  |
| `pylot add <service>`            | Add an integration (google-calendar, telegram, whatsapp, github, slack)         |
| `pylot remove <service>`         | Remove an integration                                                           |
| `pylot doctor`                   | Diagnostic checks                                                               |
| `pylot status`                   | Show agent status and connected services                                        |
| `pylot tools`                    | List available tools                                                            |
| `pylot telegram-bot`             | Start Telegram bot mode                                                         |
| `pylot serve`                    | Start background daemon with scheduler                                          |
| `pylot serve install`            | Install as system service (launchd / systemd)                                   |
| `pylot serve uninstall`          | Remove system service                                                           |
| `pylot jobs list`                | List scheduled jobs                                                             |
| `pylot jobs run <name>`          | Run a job immediately                                                           |
| `pylot jobs enable <name>`       | Enable a job                                                                    |
| `pylot jobs disable <name>`      | Disable a job                                                                   |
| `pylot config list`              | Show current configuration                                                      |
| `pylot config set <key> <value>` | Update a configuration value                                                    |
| `pylot logs`                     | Tail agent logs (`--scheduler` for scheduler logs)                              |

---

## Usage — Python SDK

### Install

```bash
pip install pylot
```

> The Rust binary must also be on your `PATH`. The Python package wraps the native binary via PyO3.

### Chat

```python
from pylot import PylotAgent

agent = PylotAgent.from_config("~/.pylot/secrets.enc")
response = agent.chat("What meetings do I have today?")
print(response)
```

### Programmatic Configuration

```python
from pylot import PylotAgent, Config

config = Config(
    llm_provider="openai",
    llm_model="gpt-4o",
    openai_api_key="sk-...",
    telegram_bot_token="...",
)
agent = PylotAgent(config)
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

See [python/README.md](python/README.md) for full Python documentation.

---

## Usage — Node.js SDK

### Install

```bash
npm install pylot
```

> The Rust binary must also be on your `PATH`. The Node.js package wraps the native binary via NAPI-RS.

### Chat

```typescript
import { PylotAgent } from 'pylot';

const agent = await PylotAgent.fromConfig('~/.pylot/secrets.enc');
const response = await agent.chat('What meetings do I have today?');
console.log(response);
```

### Programmatic Configuration

```typescript
import { PylotAgent, Config } from 'pylot';

const config: Config = {
  llmProvider: 'anthropic',
  llmModel: 'claude-sonnet-4-20250514',
  anthropicApiKey: process.env.ANTHROPIC_API_KEY,
};

const agent = new PylotAgent(config);
const response = await agent.chat('Set a reminder for 5pm to review PRs');
console.log(response);
```

### Diagnostics

```typescript
await PylotAgent.doctor();
await PylotAgent.status();
```

---

## Configuration

OpenPylot uses a layered configuration system (highest to lowest priority):

1. **Environment variables**
2. **Encrypted secrets vault** (`~/.pylot/secrets.enc`)
3. **TOML config files** (`config/default.toml` or `~/.pylot/config.toml`)
4. **Built-in defaults**

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the full environment variable reference, TOML options, and secrets vault usage.

### Quick Configuration

```toml
# config/default.toml
[agent]
name = "My Assistant"
persona = "You are a helpful personal assistant."

[llm]
provider = "openai"     # or "anthropic"
model = "gpt-4o"

[memory]
enabled = true

[social]
twitter_enabled = true
facebook_enabled = true
```

---

## Integrations

### Google Calendar & Gmail

```bash
pylot add google-calendar
```

Requires OAuth 2.0 credentials from [Google Cloud Console](https://console.cloud.google.com/). See [docs/INSTALLATION.md](docs/INSTALLATION.md#google-calendar) for step-by-step setup.

### Telegram Bot

```bash
pylot add telegram
pylot telegram-bot     # Start bot mode
```

Create a bot via [@BotFather](https://t.me/botfather). Bot commands: `/start`, `/help`, `/tools`, `/clear`.

### WhatsApp (via Twilio)

```bash
pylot add whatsapp
```

Requires a [Twilio account](https://console.twilio.com/) with WhatsApp sandbox or business number.

### Social Media (17 Platforms)

OpenPylot supports publishing, deleting, and analytics for 17 social media platforms. Each platform auto-enables when credentials are detected.

| Platform  | Auth Method       | Key Env Vars                                                                                   |
| --------- | ----------------- | ---------------------------------------------------------------------------------------------- |
| Twitter/X | OAuth 1.0a        | `TWITTER_API_KEY`, `TWITTER_API_SECRET`, `TWITTER_ACCESS_TOKEN`, `TWITTER_ACCESS_TOKEN_SECRET` |
| LinkedIn  | OAuth 2.0         | `LINKEDIN_ACCESS_TOKEN`, `LINKEDIN_PERSON_ID`                                                  |
| Bluesky   | App password      | `BLUESKY_HANDLE`, `BLUESKY_APP_PASSWORD`                                                       |
| Facebook  | Page token        | `FACEBOOK_ACCESS_TOKEN`, `FACEBOOK_PAGE_ID`                                                    |
| Instagram | FB Graph API      | `INSTAGRAM_ACCESS_TOKEN`, `INSTAGRAM_USER_ID`                                                  |
| TikTok    | OAuth 2.0         | `TIKTOK_ACCESS_TOKEN`                                                                          |
| YouTube   | OAuth 2.0         | `YOUTUBE_ACCESS_TOKEN`                                                                         |
| Pinterest | OAuth 2.0         | `PINTEREST_ACCESS_TOKEN`, `PINTEREST_BOARD_ID`                                                 |
| Reddit    | OAuth 2.0         | `REDDIT_ACCESS_TOKEN`, `REDDIT_SUBREDDIT`                                                      |
| Threads   | Meta Graph API    | `THREADS_ACCESS_TOKEN`, `THREADS_USER_ID`                                                      |
| Mastodon  | App token         | `MASTODON_ACCESS_TOKEN`, `MASTODON_INSTANCE`                                                   |
| Discord   | Bot / webhook     | `DISCORD_BOT_TOKEN`, `DISCORD_CHANNEL_ID`                                                      |
| Slack     | Bot token         | `SLACK_BOT_TOKEN`, `SLACK_CHANNEL`                                                             |
| Medium    | Integration token | `MEDIUM_TOKEN`                                                                                 |
| Dev.to    | API key           | `DEVTO_API_KEY`                                                                                |
| Hashnode  | API key           | `HASHNODE_API_KEY`, `HASHNODE_PUBLICATION_ID`                                                  |
| WordPress | Basic auth        | `WORDPRESS_SITE_URL`, `WORDPRESS_USERNAME`, `WORDPRESS_APP_PASSWORD`                           |

See [docs/SOCIAL-PLATFORMS.md](docs/SOCIAL-PLATFORMS.md) for detailed per-platform setup guides.

---

## Advanced Features

### Smart Memory

SQLite-backed semantic memory system with OpenAI embeddings:

- **Personal memory** — Auto-extracted facts from conversations ("User prefers morning meetings")
- **Knowledge base** — Upload documents, chunked and embedded for semantic search
- **Configurable** — Similarity threshold, chunk size, extraction interval

```toml
[memory]
enabled = true
embedding_model = "text-embedding-3-small"
auto_extract = true
similarity_threshold = 0.35
```

### Skills System

Declarative SKILL.md files with YAML frontmatter. Matched to user intents at runtime via embedding similarity and injected into the agent's context.

```markdown
---
name: email-drafting
description: Draft professional emails
triggers:
  - draft an email
  - write an email
---

When drafting emails:

1. Ask for recipient, subject, and key points
2. Use a professional tone unless told otherwise
3. Keep paragraphs short
```

### Sub-Agents

Spawn specialist sub-agents with isolated context and tool subsets:

- **Researcher** — Web search, document analysis
- **Coder** — Code generation, review
- **Marketing** — Campaign planning, content generation, social media publishing

### MCP (Model Context Protocol)

Connect external tool servers via JSON-RPC:

```toml
[mcp]
enabled = true
# config_path = "~/.pylot/mcp-servers.json"
```

### Learning Engine

- **Auto-scoring** — LLM-as-judge rates response quality (configurable vote count, majority wins)
- **Prompt evolution** — Automatically adjusts system prompts based on feedback patterns
- **Skill evolution** — When success rate drops below 40%, auto-generates new SKILL.md files from failure analysis

```toml
[learning]
enabled = true
auto_score = false
judge_votes = 3
skill_evolution = false
```

### Marketing Agent

A specialist sub-agent for social media automation:

- Create content strategies with target platforms and tone
- Generate platform-specific content drafts
- Review and approve content before publishing
- Track performance across platforms

---

## Web Dashboard

OpenPylot ships with a Next.js web UI.

### Starting the Web UI

```bash
# 1. Start the backend API server (port 3001)
pylot serve

# 2. In another terminal, start the frontend (port 3000)
cd frontend && npm install && npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

### Pages

| Page               | Path         | Description                                      |
| ------------------ | ------------ | ------------------------------------------------ |
| **Home**           | `/`          | Landing page with quick links                    |
| **Chat**           | `/chat`      | Real-time chat with the agent via WebSocket      |
| **Integrations**   | `/setup`     | Connect & disconnect services                    |
| **Knowledge Base** | `/knowledge` | Manage documents, upload text, and search        |
| **Dashboard**      | `/dashboard` | Agent status, scheduled jobs, recent logs        |
| **Settings**       | `/settings`  | Agent config, model selection, memory management |

### Connecting Integrations via the Web UI

1. Navigate to **Integrations** (`/setup`)
2. Click **Connect** on a service
3. For **Telegram / WhatsApp / GitHub / Slack**: enter your tokens in the credential modal
4. For **Google Calendar / Gmail**: the backend starts an OAuth flow and opens the browser
5. Use **Test** to verify connectivity, **Disconnect** to remove credentials

Integrations configured via the CLI (`pylot init` or `pylot add`) are automatically visible in the web UI.

### Building for Production

```bash
cd frontend && npm run build   # Generates static files in frontend/out/
```

---

## Background Service & Scheduler

### Scheduled Jobs

| Job                | Default Schedule | Description                            |
| ------------------ | ---------------- | -------------------------------------- |
| `reminder_check`   | Every 1 min      | Check and fire due reminders           |
| `rsvp_monitor`     | Every 15 min     | Detect RSVP changes on calendar events |
| `meeting_reminder` | Every 5 min      | Send upcoming meeting notifications    |
| `calendar_sync`    | Every 30 min     | Sync calendar events                   |
| `token_refresh`    | Every 45 min     | Refresh OAuth tokens                   |
| `daily_briefing`   | 8:00 AM          | Morning summary                        |
| `email_digest`     | 6:00 PM          | Evening email digest                   |

### Install as System Service

```bash
# macOS (launchd)
pylot serve install

# Linux (systemd)
pylot serve install
```

### Manage

```bash
pylot status            # Check status
pylot logs              # Tail agent logs
pylot jobs list         # List scheduled jobs
pylot jobs run <name>   # Run a job now
pylot serve uninstall   # Remove service
```

---

## Docker

```bash
# Docker Compose (recommended)
docker compose up -d

# Or build manually
docker build -t pylot .
docker run --rm -it \
  -v ~/.pylot:/home/pylot/.pylot \
  -e OPENAI_API_KEY=sk-... \
  -p 3001:3001 \
  -p 8443:8443 \
  pylot
```

---

## API Reference

The backend API (default `http://localhost:3001`) exposes:

| Method   | Endpoint                              | Description                         |
| -------- | ------------------------------------- | ----------------------------------- |
| `GET`    | `/api/status`                         | Agent status                        |
| `GET`    | `/api/integrations`                   | List integrations with vault status |
| `POST`   | `/api/integrations/{service}/connect` | Connect a service                   |
| `DELETE` | `/api/integrations/{service}`         | Disconnect                          |
| `POST`   | `/api/integrations/{service}/test`    | Test connectivity                   |
| `GET`    | `/api/knowledge/collections`          | List collections                    |
| `POST`   | `/api/knowledge/collections`          | Create collection                   |
| `GET`    | `/api/knowledge/documents`            | List documents                      |
| `POST`   | `/api/knowledge/documents`            | Upload document                     |
| `POST`   | `/api/knowledge/search`               | Search documents                    |
| `GET`    | `/api/jobs`                           | List scheduled jobs                 |
| `PATCH`  | `/api/jobs/{name}`                    | Update job (enable/disable)         |
| `POST`   | `/api/jobs/{name}/run`                | Run job immediately                 |
| `GET`    | `/api/settings`                       | Get settings                        |
| `PATCH`  | `/api/settings`                       | Update settings                     |
| `GET`    | `/api/memory`                         | List memory facts                   |
| `GET`    | `/api/logs`                           | Recent logs (`?level=`, `?limit=`)  |
| `WS`     | `/ws/chat`                            | WebSocket for real-time chat        |
| `WS`     | `/ws/notifications`                   | WebSocket for push notifications    |

---

## Project Structure

```
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── lib.rs               # Library crate (re-exports all modules)
│   ├── agent.rs             # Agent loop: LLM ↔ tool calls
│   ├── config.rs            # Layered config (env > vault > TOML > defaults)
│   ├── context.rs           # Conversation context management
│   ├── document_chunker.rs  # Document chunking for knowledge base
│   ├── memory.rs            # Persistent memory store (JSON)
│   ├── smart_memory.rs      # SQLite + embeddings semantic memory
│   ├── secrets.rs           # AES-256-GCM encrypted vault
│   ├── traits.rs            # Core traits
│   ├── init.rs              # Setup wizard, doctor, status
│   ├── terminal.rs          # Interactive REPL
│   ├── scheduler.rs         # Tokio cron scheduler
│   ├── oauth.rs             # Browser-based OAuth 2.0 flows
│   ├── telegram_bot.rs      # Telegram long-polling bot
│   ├── api/                 # Axum REST API + WebSocket handlers
│   ├── llm/                 # LLM provider trait + OpenAI, Anthropic
│   ├── tools/               # Tool registry + 8 built-in tools
│   ├── webhooks/            # Webhook endpoint handlers
│   ├── jobs/                # Background job definitions
│   ├── skills/              # Skill system (SKILL.md loader, matcher)
│   ├── memory_v2/           # Memory v2 (structured memory types)
│   ├── streaming/           # Token streaming (WebSocket, SSE)
│   ├── sub_agents/          # Sub-agent orchestration
│   ├── mcp/                 # Model Context Protocol client
│   ├── learning/            # Auto-scorer, prompt evolution, skill evolver
│   ├── social/              # Social media manager (17 providers)
│   └── marketing/           # Marketing agent (campaigns, content)
├── frontend/                # Next.js 15 web dashboard
├── python/                  # Python SDK (PyO3 + maturin)
├── node/                    # Node.js SDK (NAPI-RS)
├── config/default.toml      # Default TOML configuration
├── docs/                    # Documentation
├── plan_docs/               # Feature planning documents (00-13)
├── tests/                   # Test suites (Rust, Python, Node.js)
├── Formula/pylot.rb         # Homebrew formula
├── install.sh               # One-line installer
├── Dockerfile
├── docker-compose.yml
├── CONTRIBUTING.md
├── CHANGELOG.md
└── Cargo.toml
```

### Data Storage

```
~/.pylot/
├── bin/pylot               # Binary (if installed via installer)
├── secrets.enc             # Encrypted secrets vault
├── config.toml             # User config overrides
└── data/
    ├── notes.json
    ├── reminders.json
    ├── memory.json
    ├── smart_memory.db     # SQLite semantic memory
    ├── google_tokens.json
    ├── gmail_tokens.json
    ├── knowledge_collections.json
    ├── knowledge_documents.json
    ├── history.txt
    └── conversations/
```

---

## Development

### Prerequisites

- **Rust 1.75+** — [rustup.rs](https://rustup.rs/)
- **Python 3.9+** and **maturin** — for Python bindings
- **Node.js 18+** and **@napi-rs/cli** — for Node.js bindings

### Build & Test

```bash
cargo build --release
cargo test                # 100 tests passing

cd python && maturin develop && pytest
cd node && npm run build && npm test
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
        Ok(ToolResult::ok("Done!"))
    }
}
```

3. Register in `src/main.rs`

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, commit conventions, and PR guidelines.

---

## Troubleshooting

| Problem                        | Solution                                                                  |
| ------------------------------ | ------------------------------------------------------------------------- |
| "No LLM API key configured"    | Run `pylot init` or set `OPENAI_API_KEY` / `ANTHROPIC_API_KEY`            |
| "Secrets file is corrupted"    | Back up `~/.pylot/secrets.enc` and re-run `pylot init`                    |
| Google OAuth fails             | Ensure port 8085 is free (`lsof -i :8085`), or set `GOOGLE_REDIRECT_PORT` |
| Binary not found (Python/Node) | The Rust binary must be on your `PATH`                                    |
| Debug logging                  | `RUST_LOG=debug pylot`                                                    |

Run `pylot doctor` to diagnose issues automatically.

---

## Documentation

Full docs live in [`docs/`](docs/README.md). Highlights:

| Document                                             | Description                          |
| ---------------------------------------------------- | ------------------------------------ |
| [docs/README.md](docs/README.md)                     | Documentation index                  |
| [docs/GETTING-STARTED.md](docs/GETTING-STARTED.md)   | 5-minute quickstart                  |
| [docs/INSTALLATION.md](docs/INSTALLATION.md)         | Full installation guide              |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md)       | Configuration reference              |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)         | System architecture                  |
| [docs/API.md](docs/API.md)                           | REST + WebSocket API reference       |
| [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md)             | Docker, systemd, production          |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)           | Build, test, contribute              |
| [docs/SECURITY.md](docs/SECURITY.md)                 | Security model                       |
| [docs/AGENTS.md](docs/AGENTS.md)                     | Sub-agents                           |
| [docs/PLUGINS.md](docs/PLUGINS.md)                   | Plug-and-play skills & agent presets |
| [docs/SOCIAL-PLATFORMS.md](docs/SOCIAL-PLATFORMS.md) | Social media platform setup          |
| [CONTRIBUTING.md](CONTRIBUTING.md)                   | Contribution guidelines              |
| [CHANGELOG.md](CHANGELOG.md)                         | Version history                      |

---

## License

MIT
