# OpenPylot — Architecture

Overview of the system architecture, module relationships, and design decisions.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        User Interfaces                          │
│   CLI (REPL)  │  Telegram Bot  │  Web Dashboard  │  SDKs       │
└───────┬───────┴───────┬────────┴───────┬─────────┴──────┬──────┘
        │               │                │                │
        ▼               ▼                ▼                ▼
┌─────────────────────────────────────────────────────────────────┐
│                         Agent Core                              │
│   agent.rs — LLM ↔ Tool loop, conversation orchestration        │
│   context.rs / context_builder.rs — Message history management  │
│   traits.rs — Shared trait definitions                          │
└───────┬───────────┬──────────┬────────────┬─────────────────────┘
        │           │          │            │
   ┌────▼────┐ ┌───▼───┐ ┌───▼────┐ ┌─────▼─────┐
   │   LLM   │ │ Tools │ │ Memory │ │ Skills    │
   │ Providers│ │Registry│ │ System │ │ Matcher   │
   └─────────┘ └───────┘ └────────┘ └───────────┘
        │           │          │            │
        ▼           ▼          ▼            ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Extension Layer                            │
│  Sub-Agents  │  MCP  │  Learning  │  Social  │  Marketing      │
└───────┬──────┴───┬───┴─────┬──────┴────┬─────┴───────┬─────────┘
        │          │         │           │             │
        ▼          ▼         ▼           ▼             ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Infrastructure                              │
│  Config  │  Secrets Vault  │  Scheduler  │  OAuth  │  Webhooks  │
└─────────────────────────────────────────────────────────────────┘
```

## Module Overview

### Entrypoints

| Module | File | Purpose |
|--------|------|---------|
| **CLI** | `main.rs` | Clap-based CLI with subcommands (`init`, `chat`, `serve`, `tools`, etc.) |
| **Terminal** | `terminal.rs` | Interactive REPL with history, `/` commands |
| **Telegram** | `telegram_bot.rs` | Long-polling Telegram bot with slash commands |
| **API** | `api/` | Axum REST API + WebSocket endpoints for web dashboard |

### Agent Core

| Module | File | Purpose |
|--------|------|---------|
| **Agent** | `agent.rs` | Central orchestrator. Sends messages to the LLM, parses tool calls, executes tools, loops until final response. |
| **Context** | `context.rs` | Manages conversation message history with role-based messages. |
| **Context Builder** | `context_builder.rs` | Constructs the full context (system prompt + memory + skills + messages). |
| **Traits** | `traits.rs` | Shared trait definitions used across modules. |

### LLM Layer

| Module | File | Purpose |
|--------|------|---------|
| **LLM trait** | `llm/mod.rs` | `LlmProvider` trait — `chat()` with messages and tools. |
| **OpenAI** | `llm/openai.rs` | OpenAI API client (GPT-4o, streaming support). |
| **Anthropic** | `llm/anthropic.rs` | Anthropic API client (Claude, streaming support). |

Providers are swappable at runtime via config. Both support function calling and streaming.

### Tool System

| Module | File | Purpose |
|--------|------|---------|
| **Registry** | `tools/mod.rs` | `Tool` trait, `ToolRegistry` for dynamic dispatch, JSON Schema definitions. |
| **Calendar** | `tools/calendar.rs` | Google Calendar: list events, create events, create meetings with Meet links. |
| **Gmail** | `tools/gmail.rs` | Gmail: search, read, send, reply, draft management. |
| **Notes** | `tools/notes.rs` | Local notes: create, list, search, delete. |
| **Reminders** | `tools/reminder.rs` | Local reminders: set, list, complete, delete. |
| **Telegram** | `tools/telegram.rs` | Send/receive Telegram messages. |
| **WhatsApp** | `tools/whatsapp.rs` | Send WhatsApp messages via Twilio. |
| **Memory** | `tools/memory.rs` | Store, search, list memory facts. |
| **Knowledge** | `tools/knowledge.rs` | Document upload, chunking, and semantic search. |

### Memory System

| Module | File | Purpose |
|--------|------|---------|
| **Memory (v1)** | `memory.rs` | JSON-file-based conversation memory. |
| **Smart Memory** | `smart_memory.rs` | SQLite + OpenAI embeddings. Stores facts, knowledge chunks. Cosine similarity search. |
| **Memory v2** | `memory_v2/` | Structured typed memory (personal, episodic, semantic). |

**Data flow**: Conversations → auto-extraction (every N turns) → embedding → SQLite → context injection (top-K similar facts per query).

### Skills System

| Module | File | Purpose |
|--------|------|---------|
| **Skills** | `skills/` | Load SKILL.md files, parse YAML frontmatter, embed descriptions, match to user intents. |

Skills are declarative Markdown files. At query time, the user's message is embedded and compared against skill embeddings. Matching skills are injected into the system prompt.

### Extension Layer

| Module | File | Purpose |
|--------|------|---------|
| **Sub-Agents** | `sub_agents/` | Spawn isolated agents with subset tools, own LLM config, and scoped context. Types: researcher, coder, marketing. |
| **MCP** | `mcp/` | Model Context Protocol client. Connects to external tool servers via JSON-RPC over stdio/SSE. |
| **Learning** | `learning/` | Auto-scorer (LLM-as-judge, majority vote), prompt evolution, skill evolver (failure → new SKILL.md). |
| **Social** | `social/` | Social media manager with 17 platform providers. Publish, delete, analytics. |
| **Marketing** | `marketing/` | Campaign planning, content strategy, content generation with approval workflow. |
| **Streaming** | `streaming/` | Token streaming over WebSocket and SSE. Handles partial messages and tool call deltas. |

### Infrastructure

| Module | File | Purpose |
|--------|------|---------|
| **Config** | `config.rs` | Layered config: env vars > secrets vault > TOML file > defaults. |
| **Secrets** | `secrets.rs` | AES-256-GCM vault with Argon2id KDF, machine-bound encryption key. |
| **Scheduler** | `scheduler.rs` | Tokio-based cron scheduler. Runs background jobs on configurable intervals. |
| **OAuth** | `oauth.rs` | Browser-based OAuth 2.0 flows (Google Calendar, Gmail). Local redirect server on configurable port. |
| **Webhooks** | `webhooks/` | Incoming webhook handlers (Google Calendar push, Gmail push, GitHub, Slack). |
| **Jobs** | `jobs/` | Background job definitions: RSVP monitor, meeting reminders, calendar sync, token refresh, daily briefing, email digest. |
| **Document Chunker** | `document_chunker.rs` | Splits documents into overlapping chunks for embedding and semantic search. |

## Design Decisions

### Single Binary

OpenPylot compiles to a single static binary. All functionality (CLI, API server, scheduler, Telegram bot) is included. This simplifies deployment and eliminates runtime dependency management.

### Layered Configuration

Config resolution order (highest wins):
1. Environment variables
2. Encrypted secrets vault
3. TOML config files
4. Built-in defaults

This allows secrets to stay encrypted at rest while being overridable for CI/CD or Docker environments.

### Tool Loop Architecture

The agent uses a synchronous tool loop:

```
User message
    → LLM call (with tool definitions)
    → Parse response
    → If tool call: execute tool → append result → loop back to LLM
    → If text response: return to user
```

Each iteration appends the tool result to the conversation context, so the LLM can chain multiple tool calls within a single turn.

### Memory Architecture

Two-tier memory system:

1. **Short-term**: Conversation context (last N messages, configurable).
2. **Long-term**: SQLite with embeddings. Auto-extracted facts from conversations are embedded and stored. At query time, relevant facts are retrieved via cosine similarity and injected into context.

### Security Model

- Secrets are encrypted at rest using AES-256-GCM.
- Key derivation uses Argon2id with machine-specific salt.
- OAuth tokens are stored in the vault, never in plaintext files.
- The vault is machine-bound — moving the file to another machine requires re-authentication.

## Data Flow

### Chat Request

```
User Input
  ├── Context Builder
  │     ├── System prompt
  │     ├── Memory search (top-K similar facts)
  │     ├── Skill matching (embed → cosine similarity)
  │     └── Conversation history (last N messages)
  │
  ├── LLM Provider (OpenAI / Anthropic)
  │     └── Response (text or tool calls)
  │
  ├── Tool Execution (if tool call)
  │     └── Tool result → append to context → re-call LLM
  │
  └── Final Response → User
        └── Memory extraction (async, every N turns)
```

### Social Media Publishing

```
User Request ("post this to Twitter and LinkedIn")
  ├── Agent identifies platforms
  ├── Social Manager
  │     ├── ContentType detection (text, image, video, article)
  │     ├── Per-platform provider
  │     │     ├── Twitter → OAuth 1.0a → Twitter API v2
  │     │     └── LinkedIn → OAuth 2.0 → LinkedIn API
  │     └── Results aggregated
  └── Response to user
```

### Background Scheduler

```
Scheduler (tokio cron)
  ├── reminder_check (1 min)  → Notes store → Telegram/notification
  ├── rsvp_monitor (15 min)   → Calendar API → Detect changes → Notify
  ├── meeting_reminder (5 min) → Calendar API → Notify upcoming
  ├── calendar_sync (30 min)  → Calendar API → Local cache
  ├── token_refresh (45 min)  → OAuth refresh → Vault update
  ├── daily_briefing (8:00)   → Calendar + Gmail → LLM summary → Notify
  └── email_digest (18:00)    → Gmail search → LLM summary → Notify
```

## Dependency Graph (Simplified)

```
main.rs
  ├── agent (core loop)
  │     ├── llm (providers)
  │     ├── tools (registry)
  │     ├── context / context_builder
  │     ├── smart_memory
  │     ├── skills
  │     ├── streaming
  │     └── sub_agents
  ├── config
  │     └── secrets
  ├── scheduler
  │     └── jobs
  ├── api
  │     └── webhooks
  ├── init / terminal
  ├── telegram_bot
  ├── mcp
  ├── learning
  ├── social
  └── marketing
```

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime |
| `axum` | HTTP/WebSocket server |
| `reqwest` | HTTP client |
| `rusqlite` | SQLite (smart memory) |
| `serde` / `serde_json` | Serialization |
| `clap` | CLI argument parsing |
| `chrono` | Date/time handling |
| `uuid` | Unique identifiers |
| `aes-gcm` / `argon2` | Encryption |
| `async-trait` | Async trait support |
| `pyo3` | Python bindings |
| `napi` / `napi-derive` | Node.js bindings |
