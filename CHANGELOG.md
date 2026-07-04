# Changelog

All notable changes to OpenPylot are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.1.0] — 2026-07-04

Initial public release.

### Core

- **Agent core** — LLM ↔ tool-call loop with OpenAI and Anthropic providers (hot-swappable).
- **CLI** (`pylot`) — interactive REPL with `/slash` commands and rustyline autocomplete, plus one-shot `chat`, and `add`, `remove`, `doctor`, `status`, `tools`, `serve`, `jobs`, `config`, `logs`, `agents` subcommands.
- **Configuration** — layered config: environment variables > secrets vault > TOML > defaults.
- **Encrypted secrets vault** — AES-256-GCM with Argon2id KDF, machine-bound. Keys can be set interactively from the terminal on first run, or from the web dashboard setup wizard (no `.env` required).
- **Setup wizard** — `pylot init` interactive setup with doctor diagnostics.

### Memory & Skills

- **Smart Memory** — SQLite-backed semantic memory with OpenAI embeddings; auto-extracts personal facts and injects relevant context via cosine similarity. Structured memory types (personal, episodic, semantic).
- **Persistent conversation memory** — JSON-based history.
- **Skills system** — declarative SKILL.md files with YAML frontmatter, matched to user intents at runtime and injected into the system prompt. Bundled skills across productivity, coding, communication, research, media, and system categories.
- **Sub-agents** — spawn specialist sub-agents with isolated context and configurable tool access. Plug-and-play `.toml` agent presets (`coder`, `researcher`, `writer`, `marketer`) loaded from `agents/`, `~/.pylot/agents/`, or workspace `./agents/` — no rebuild required.
- **Learning engine** — LLM-as-judge auto-scoring (majority vote), prompt evolution, and automatic skill generation from failure patterns.

### Integrations

- **Google Calendar** — OAuth 2.0 login, list/create events, create meetings with Google Meet links.
- **Gmail** — search, read, send, reply, draft create/send/delete.
- **Telegram** — long-polling bot with slash commands.
- **WhatsApp** — send messages via Twilio.
- **Social media** — 17 platforms: Twitter/X, LinkedIn, Bluesky, Facebook, Instagram, TikTok, YouTube, Pinterest, Reddit, Threads, Mastodon, Discord, Slack, Medium, Dev.to, Hashnode, WordPress.
- **Marketing agent** — campaign planning, content strategy generation, and content creation with approval workflow.
- **Knowledge base** — document upload, chunking, collection management, and semantic search.
- **MCP support** — Model Context Protocol client for connecting external tool servers via JSON-RPC.

### Server & Platform

- **Web dashboard** — Next.js frontend with real-time chat (WebSocket), integrations, knowledge base, and settings.
- **REST API** — Axum-based server with endpoints for status, chat, integrations, knowledge base, jobs, settings, memory, and setup.
- **Streaming** — token-by-token responses over WebSocket and SSE.
- **Background scheduler** — cron-based jobs: RSVP monitor, meeting reminders, calendar sync, token refresh, daily briefing, email digest.
- **Webhooks** — incoming handlers for Google Calendar, Gmail, GitHub, and Slack.
- **System service** — `pylot serve install` for launchd (macOS) and systemd (Linux).
- **Notes & reminders** — create, list, search, delete; stored locally.

### SDKs & Distribution

- **Python SDK** (PyO3) — in-process Rust agent core, `PylotMemory`, `PylotSkills`, `PylotLearning`.
- **Node.js SDK** (NAPI) — in-process agent core with typed TypeScript structs.
- **Docker** — Dockerfile and docker-compose.yml.
- **Homebrew** — `brew tap gmvofficial/tap && brew install openpylot`.
- **One-line installer** — `curl | bash` for macOS and Linux.
