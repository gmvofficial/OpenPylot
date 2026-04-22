# Changelog

All notable changes to OpenPylot are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/).

## [1.0.0-rc1] — 2025-01-XX

### Added

- **Plug-and-play agent presets** — drop `.toml` manifests in `agents/`, `~/.pylot/agents/`, or your workspace `./agents/` to register new sub-agents with custom personas, models, tools, and limits — no rebuild required. Four bundled presets: `coder`, `researcher`, `writer`, `marketer`.
- **Agent preset CLI** — `pylot agents presets | show <name> | path | spawn --preset <name> <task>`.
- **Agent preset HTTP API** — `GET /api/agents/presets`, `GET /api/agents/presets/{name}`.
- **Agent preset picker in Web UI** — the Sub-Agents page shows all available presets and prefills model/tools on spawn.
- **29 new skills ported from OpenClaw** — across productivity, coding, communication, research, media, and system categories. All rewritten to OpenPylot's flat YAML schema.
- **Terminal UX polish** — rustyline-based autocomplete for all `/slash` commands, and a braille spinner that plays while the agent is thinking in non-streaming mode.
- **Python SDK (PyO3)** — re-synced to current core API: `chat()` runs the real Rust `Agent` in-process, `PylotMemory` uses the real `memory_v2::MemoryStore`. `cargo check` clean.
- **Node.js SDK (NAPI)** — `chat()` converted from shell-out to in-process Rust agent core. Memory/Skills/Learning return typed `#[napi(object)]` structs for clean TypeScript types. `cargo check` clean.
- **`docs/PLUGINS.md`** — how to author agent presets and SKILL.md files, precedence rules, and examples.

### Changed

- **Rebrand: `GMV Agent` → `OpenPylot`** across the Web UI (sidebar, header, metadata, setup wizard), frontend package name, and TS type comments.
- **Frontend version bumped** to `1.0.0` (`openpylot-frontend`).

### Removed

- `src/context_builder.rs` (unused `ContextBuilder` struct — verified zero references before deletion).
- `README.md.old` (stale duplicate).

### Documentation

- Added `docs/PLUGINS.md` covering agent manifests, SKILL.md authoring, precedence chain, and location conventions.

---

## [0.3.0] — 2025-01-XX

### Added

- **Social media** — 14 new platform providers (Facebook, Instagram, TikTok, YouTube, Pinterest, Reddit, Threads, Mastodon, Discord, Slack, Medium, Dev.to, Hashnode, WordPress) joining existing Twitter, LinkedIn, and Bluesky for a total of 17 platforms.
- **Smart Memory** — SQLite-backed semantic memory with OpenAI embeddings. Auto-extracts personal facts from conversations and injects relevant memory into context via cosine similarity search.
- **Skills system** — Declarative SKILL.md files with YAML frontmatter. Skills are embedded and matched to user intents at runtime and injected into the system prompt.
- **Sub-agents** — Spawn specialist sub-agents (researcher, coder, marketing) with isolated context and configurable tool access.
- **Streaming** — Token-by-token streaming over WebSocket and SSE for responsive chat experiences.
- **MCP support** — Model Context Protocol client for connecting external tool servers via JSON-RPC.
- **Learning engine** — LLM-as-judge auto-scoring (majority vote across configurable judge count), prompt evolution, and automatic skill generation from failure patterns.
- **Marketing agent** — Campaign planning, content strategy generation, and content creation with approval workflow.
- **Memory v2** — Structured memory types (personal, episodic, semantic).
- **ContentType enum** — Support for text, image, video, article, story, reel, pin, and thread content across social platforms.
- **Full config wiring** — All 17 social platforms, MCP, learning, and marketing are fully configurable via environment variables, secrets vault, and TOML config.
- **100 library tests** passing across all modules.

### Changed

- Platform enum expanded from 5 to 17 variants.
- SocialPost struct expanded with platform-specific fields.
- Config system updated to support all new modules.

## [0.2.0] — 2024-12-XX

### Added

- **Web dashboard** — Next.js 15 frontend with real-time chat (WebSocket), integrations page, knowledge base, and settings.
- **REST API** — Axum-based API server with endpoints for status, integrations, knowledge base, jobs, settings, memory, and logs.
- **WebSocket** — Real-time chat and notification channels.
- **Knowledge base** — Document upload, chunking, collection management, and semantic search.
- **Gmail integration** — Search, read, send, reply, draft create/send/delete.
- **Background scheduler** — Cron-based jobs: RSVP monitor, meeting reminders, calendar sync, token refresh, daily briefing, email digest.
- **Webhooks** — Incoming webhook handlers for Google Calendar, Gmail, GitHub, Slack.
- **System service** — `pylot serve install` for launchd (macOS) and systemd (Linux).
- **Python SDK** — PyO3 bindings with `PylotAgent`, `Config`, custom tool registration.
- **Node.js SDK** — NAPI-RS bindings with `PylotAgent`, `Config`, diagnostics.
- **Homebrew formula** — `brew tap openpylot/tap && brew install pylot`.
- **One-line installer** — `curl | bash` installer for macOS and Linux.
- **Docker support** — Dockerfile and docker-compose.yml.
- **Social media** — Twitter, LinkedIn, Bluesky publishing with provider architecture.

### Changed

- CLI expanded with `add`, `remove`, `doctor`, `status`, `tools`, `serve`, `jobs`, `config`, `logs` subcommands.
- Agent loop refactored for tool call chaining.
- Secrets vault hardened with Argon2id KDF and machine binding.

## [0.1.0] — 2024-11-XX

### Added

- **Core agent** — LLM ↔ tool call loop with OpenAI and Anthropic providers.
- **CLI** — Interactive REPL with `/clear`, `/tools`, `/help`, `/quit` commands. One-shot `chat` subcommand.
- **Configuration** — Layered config: environment variables > TOML > defaults.
- **Encrypted secrets vault** — AES-256-GCM with Argon2id KDF.
- **Google Calendar** — OAuth 2.0 login, list events, create events, create meetings with Google Meet links.
- **Telegram bot** — Long-polling bot with slash commands.
- **WhatsApp** — Send messages via Twilio.
- **Notes & reminders** — Create, list, search, delete. Stored locally in JSON.
- **Persistent memory** — JSON-based conversation memory.
- **Setup wizard** — `pylot init` interactive setup with doctor diagnostics.
