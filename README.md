# GMV Agent — Rust-Powered Personal AI Assistant

A terminal-first personal AI assistant built in Rust, inspired by OpenClaw. GMV Agent can take notes, manage your Google Calendar, send Telegram & WhatsApp messages, set reminders — all through natural language via an interactive terminal or one-shot commands.

## Features (MVP)

| Category            | Tools                                                                 |
| ------------------- | --------------------------------------------------------------------- |
| **Notes**           | Create, list, search, delete notes (stored locally as JSON)           |
| **Google Calendar** | Create events, list upcoming events, create meetings with Google Meet |
| **Telegram**        | Send messages, get bot updates                                        |
| **WhatsApp**        | Send messages via Twilio API                                          |
| **Reminders**       | Set, list, complete reminders (stored locally)                        |

**Architecture highlights:**

- **Rust-native** — zero GC, async via Tokio, fast tool execution
- **Multi-model** — supports OpenAI and Anthropic (Claude) LLMs
- **Tool-calling** — real function calling via LLM tool-use APIs (not string parsing)
- **Persistent memory** — notes, reminders, conversation history stored locally
- **Extensible** — trait-based `Tool` system for easy addition of new tools
- **Privacy-first** — all data stored on your machine, secrets never in prompts

## Prerequisites

- **Rust** ≥ 1.75 (install via [rustup](https://rustup.rs/))
- An LLM API key:
  - [OpenAI API key](https://platform.openai.com/api-keys) **or**
  - [Anthropic API key](https://console.anthropic.com/)

## Quick Start

### 1. Clone & Build

```bash
cd GMV_Agent_MVP
cp .env.example .env
# Edit .env and add your API key(s)
cargo build --release
```

### 2. Configure

Edit `.env` with your API keys:

```env
# Required: at least one LLM provider
OPENAI_API_KEY=sk-...
# or
ANTHROPIC_API_KEY=sk-ant-...
LLM_PROVIDER=anthropic

# Optional: Telegram
TELEGRAM_BOT_TOKEN=123456:ABC-DEF...
TELEGRAM_DEFAULT_CHAT_ID=your_chat_id

# Optional: WhatsApp (Twilio)
TWILIO_ACCOUNT_SID=AC...
TWILIO_AUTH_TOKEN=...
TWILIO_WHATSAPP_FROM=whatsapp:+14155238886

# Optional: Google Calendar
GOOGLE_CLIENT_ID=...apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=...
```

### 3. Run

```bash
# Interactive mode (REPL)
cargo run --release

# Or after building:
./target/release/gmv-agent

# Telegram Bot Mode (NEW! 🤖)
cargo run --release -- telegram-bot
# Now chat with the AI through Telegram instead of terminal!

# One-shot message
cargo run --release -- chat "Take a note: buy groceries"

# List configured tools
cargo run --release -- tools
```

## Telegram Bot Mode 🆕

**Chat with your AI assistant directly on Telegram!**

Instead of using the terminal, you can now message your Telegram bot and get AI responses on your phone, tablet, or any device with Telegram.

### Quick Setup:

1. Your bot is already configured in `.env` ✅
2. Start the bot:
   ```bash
   cargo run --release -- telegram-bot
   ```
3. Open Telegram and message your bot!

### Available Commands:

- `/start` - Welcome message
- `/help` - Show available commands
- `/tools` - List AI tools
- `/clear` - Clear conversation history

### Example Usage:

```
You: Take a note about the meeting tomorrow
Bot: ✅ Note created successfully!

You: Set a reminder for 3pm today
Bot: ✅ Reminder set for 3:00 PM!

You: What notes do I have?
Bot: You have 1 note:
     • About the meeting tomorrow
```

📖 **Full guide**: See [TELEGRAM_BOT.md](TELEGRAM_BOT.md) for detailed instructions.

## Google Calendar Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a project (or select existing)
3. Enable the **Google Calendar API**
4. Create **OAuth 2.0 credentials** (Desktop app type)
5. Add `http://localhost:8085` as an authorized redirect URI
6. Copy the Client ID and Client Secret to your `.env`
7. Run the setup:

```bash
cargo run --release -- setup google-calendar
```

This opens your browser for Google OAuth consent. After authorizing, tokens are stored locally in `~/.gmv-agent/data/google_tokens.json`.

## Telegram Bot Setup

1. Message [@BotFather](https://t.me/botfather) on Telegram
2. Send `/newbot` and follow the prompts
3. Copy the bot token to `TELEGRAM_BOT_TOKEN` in `.env`
4. To find your chat ID:
   - Send a message to your bot
   - Ask GMV Agent: "Get my Telegram updates"
   - The chat ID will be shown

## WhatsApp Setup (via Twilio)

1. Create a [Twilio account](https://console.twilio.com/)
2. Set up a WhatsApp sandbox (or apply for a business number)
3. Copy credentials to `.env`

## Usage Examples

```
You> Take a note about my project ideas: Build a CLI todo app and a weather dashboard
🔧 Calling tool: create_note ({"title":"Project Ideas","content":"..."})
✅ Note created successfully.

GMV Agent: I've saved your project ideas as a note titled "Project Ideas".

You> What meetings do I have today?
🔧 Calling tool: list_calendar_events ({"date":"2026-02-26"})
📅 Team Standup — 10:00 AM

GMV Agent: You have one meeting today — Team Standup at 10:00 AM.

You> Schedule a meeting with alice@example.com tomorrow at 2pm for 1 hour about Q1 planning
🔧 Calling tool: create_meeting ({"title":"Q1 Planning","start_time":"..."})
✅ Meeting created with Google Meet link

GMV Agent: Done! I've created a "Q1 Planning" meeting for tomorrow at 2:00 PM.
           Attendees: alice@example.com
           Google Meet: https://meet.google.com/xxx-xxxx-xxx

You> Set a reminder to review PRs at 5pm today
🔧 Calling tool: set_reminder ({"title":"Review PRs","remind_at":"..."})
✅ Reminder set

GMV Agent: Reminder set for today at 5:00 PM to review PRs.

You> Send a Telegram message to chat 123456: "Hey, the meeting link is ready!"
🔧 Calling tool: send_telegram_message ({"chat_id":"123456","message":"..."})
✅ Message sent

GMV Agent: Telegram message sent to chat 123456.
```

## Terminal Commands

| Command  | Description                |
| -------- | -------------------------- |
| `/clear` | Clear conversation history |
| `/tools` | List loaded tools          |
| `/help`  | Show help                  |
| `/quit`  | Exit the agent             |

## Project Structure

```
src/
├── main.rs          # CLI entry point, tool registration, system prompt
├── agent.rs         # Agent orchestrator — LLM loop with tool calling
├── config.rs        # Configuration from TOML + environment variables
├── context.rs       # Conversation context management
├── memory.rs        # Persistent memory store
├── terminal.rs      # Terminal REPL interface
├── llm/
│   ├── mod.rs       # LLM provider trait & types
│   ├── openai.rs    # OpenAI provider implementation
│   └── anthropic.rs # Anthropic (Claude) provider implementation
└── tools/
    ├── mod.rs       # Tool trait, ToolRegistry
    ├── notes.rs     # Note-taking tools (CRUD)
    ├── calendar.rs  # Google Calendar (OAuth2 + API)
    ├── telegram.rs  # Telegram Bot API
    ├── whatsapp.rs  # WhatsApp via Twilio
    └── reminder.rs  # Local reminder management
```

## Data Storage

All data is stored locally at `~/.gmv-agent/data/`:

```
~/.gmv-agent/data/
├── notes.json           # Saved notes
├── reminders.json       # Saved reminders
├── memory.json          # Agent memory (facts & summaries)
├── google_tokens.json   # Google OAuth2 tokens (if configured)
└── history.txt          # REPL command history
```

## Adding New Tools

1. Create a new file in `src/tools/` (e.g., `github.rs`)
2. Implement the `Tool` trait:

```rust
use async_trait::async_trait;
use serde_json::{json, Value};
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct MyTool { /* config fields */ }

#[async_trait]
impl Tool for MyTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "my_tool_action".into(),
            description: "What this tool does".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "param1": { "type": "string", "description": "..." }
                },
                "required": ["param1"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> anyhow::Result<ToolResult> {
        let param1 = params["param1"].as_str().unwrap_or_default();
        // ... do something ...
        Ok(ToolResult::ok("Done!"))
    }
}
```

3. Register it in `src/main.rs`:

```rust
tools.register(Box::new(my_tool::MyTool::new()));
```

## Configuration

**Environment variables** (in `.env`):

| Variable                   | Required | Description                                             |
| -------------------------- | -------- | ------------------------------------------------------- |
| `OPENAI_API_KEY`           | Yes\*    | OpenAI API key                                          |
| `ANTHROPIC_API_KEY`        | Yes\*    | Anthropic API key                                       |
| `LLM_PROVIDER`             | No       | `openai` (default) or `anthropic`                       |
| `LLM_MODEL`                | No       | Model name (default: gpt-4o / claude-sonnet-4-20250514) |
| `GOOGLE_CLIENT_ID`         | No       | Google OAuth2 client ID                                 |
| `GOOGLE_CLIENT_SECRET`     | No       | Google OAuth2 client secret                             |
| `TELEGRAM_BOT_TOKEN`       | No       | Telegram bot token                                      |
| `TELEGRAM_DEFAULT_CHAT_ID` | No       | Default Telegram chat ID                                |
| `TWILIO_ACCOUNT_SID`       | No       | Twilio account SID                                      |
| `TWILIO_AUTH_TOKEN`        | No       | Twilio auth token                                       |
| `TWILIO_WHATSAPP_FROM`     | No       | Twilio WhatsApp sender number                           |
| `AGENT_NAME`               | No       | Agent display name (default: GMV Agent)                 |
| `AGENT_PERSONA`            | No       | Agent personality description                           |

\* At least one LLM provider key is required.

**TOML config** (`config/default.toml`): Sets defaults that can be overridden by env vars.

## License

MIT
