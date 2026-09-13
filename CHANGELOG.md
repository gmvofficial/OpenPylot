# Changelog

All notable changes to OpenPylot are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [0.2.0] — 2026-09-13

A security release first, a feature release second. **If you run `pylot serve`, upgrade.**

### Security

- **The API server was unauthenticated remote code execution on your local network.** It bound
  `0.0.0.0` with `allow_origin(Any)` and no credential on any route, while the agent's `bash` tool
  was always registered and the terminal's approval gate did not exist on the web path. Anyone on
  the same Wi-Fi could POST `/api/chat` and run shell commands — and with CORS wide open and no
  token, so could any website you visited while the server was up.
  - Binds `127.0.0.1` by default; `--host` opts into a wider bind, with a warning.
  - A per-install 32-byte token, stored `0600`, required on `/api/*`, `/ws/*` and `/uploads/*`.
    Accepted as `Authorization: Bearer`, `X-Pylot-Token`, `?token=` or a cookie, compared in
    constant time. `pylot token [--url] [--rotate]` prints or revokes it.
  - CORS is an explicit origin allowlist, never `Any`.
- **Inbound webhooks were unverified**, so anyone who could reach the port could forge a GitHub or
  Slack event. Now HMAC-SHA256 with Slack's replay window enforced. A configured secret makes
  signatures required; without one, requests are accepted and startup warns by name.
- The public webhook port kept an unbounded in-memory event queue — a free denial of service.
  Capped at 500.

### Breaking

- `pylot serve` no longer listens on all interfaces, and the API now requires a token. Scripts that
  called it without one will get `401`. Run `pylot token` to get yours, or `--host 0.0.0.0` to
  restore network access (the token is still required).

### Added

- **A new terminal.** Rebuilt on `ratatui` with an inline viewport, so your shell scrollback stays
  intact. Collapsible tool cards with real status transitions, syntax-highlighted output, fuzzy
  slash completion with per-command argument completion, `@` file mentions, `Ctrl+R` history
  search, a status line carrying model/mode/spend/context pressure, and an `Esc` that genuinely
  cancels an in-flight request while keeping the partial reply. `PYLOT_CLASSIC_TUI=1` restores the
  old REPL.
- **Companion apps.** OpenPylot can host a sibling tool's own web interface: it launches the child
  on a loopback port and reverse-proxies it at `/companions/<name>/` behind its own auth.
  `pylot companions list|connect <name>`, plus a Companions page in the dashboard. OpenDbPylot is
  the first.
- **MCP in both directions.** `pylot mcp serve` exposes the assistant, its memory and its skills to
  Claude Desktop, Claude Code or any MCP host. On the consuming side, MCP now actually works (see
  Fixed) with `pylot mcp add|remove|enable|disable|test` and a management page.
- **Ten LLM providers**, up from two: OpenAI, Anthropic, Ollama, OpenRouter, Groq, Together,
  DeepSeek, Mistral, LM Studio, and any OpenAI-compatible endpoint via `llm.base_url`.
- Light / dark / system theming with no flash of the wrong theme.
- Working chat attachments: drag-and-drop, paste, per-file progress, removable chips.
- A `⌘K` command palette over actions, navigation and recent conversations.
- A stop button during streaming that keeps what already arrived.
- `companions` clients in the Python and Node SDKs.

### Fixed

- **MCP was dead code.** Startup built an empty registry and never connected anything;
  `mcp_config_path` was parsed and never read; `pylot mcp list` printed a hardcoded "No MCP servers
  configured" regardless of what was configured. It now loads `~/.pylot/mcp-servers.json` (both the
  native and Claude Desktop formats) and connects at startup — in the terminal as well as `serve`,
  which previously only `serve` did.
- **Five commands reported fabricated state.** `pylot social accounts|posts|campaigns` always
  claimed nothing was configured; `pylot config set` was a no-op that told you to run the wizard.
- **Settings saved from the web UI were silently discarded.** The writer targeted a *relative*
  `config/default.toml` and returned silently when absent — which it always is for an installed
  binary — while the handler reported success.
- Chat auto-scroll no longer yanks the view away when you have scrolled up to read, and long
  conversations no longer render every message every frame.

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
