# OpenPylot v1.0 — 30-Day Implementation Master Plan

## Executive Summary

OpenPylot (v0.3.0) is a Rust-powered personal AI assistant with 21+ tools, smart memory (SQLite + embeddings), multi-channel access (CLI, API, WebSocket, Telegram), and encrypted secrets vault. This plan transforms it from MVP → market-ready product by adding the critical features found in competitors (OpenClaw, IronClaw, MetaClaw, Claw Code, Postiz).

---

## Current State Assessment

### What We Have (Strengths)
- ✅ Solid Rust core with Tokio async runtime
- ✅ 21+ tools (Calendar, Gmail, Notes, Reminders, GitHub, Slack, etc.)
- ✅ Smart Memory with SQLite + OpenAI embeddings
- ✅ AES-256-GCM encrypted secrets vault
- ✅ Multi-channel: CLI REPL, REST API, WebSocket, Telegram
- ✅ OAuth 2.0 flows (Google, GitHub, Slack)
- ✅ Background scheduler (cron-based jobs)
- ✅ Webhooks (Google Calendar, Gmail, GitHub, Slack)
- ✅ Document processing (PDF, DOCX, Excel, CSV, HTML)
- ✅ Setup wizard (`pylot init`)
- ✅ 170+ tests (78 Rust + 40 Python + 53 Node)
- ✅ Docker + Homebrew + pip + npm install paths
- ✅ MemoryProvider trait abstraction (designed, not yet used)
- ✅ Tool trait + registry system

### What We're Missing (Gaps)

| Gap | Priority | Competitor Reference |
|-----|----------|---------------------|
| Plugin/Skill System | P0 | OpenClaw SKILL.md, IronClaw skills/ |
| Sub-Agent System | P0 | OpenClaw sessions_spawn, IronClaw jobs |
| Advanced Memory Management | P0 | MetaClaw 6-type memory, IronClaw hybrid search |
| RL-Based Learning | P1 | MetaClaw GRPO, IronClaw self-improvement |
| Traditional Learning | P1 | IronClaw prompt evolution, skill extraction |
| Social Media Manager Agent | P1 | Postiz 36+ platforms |
| Complete Python SDK (PyO3) | P1 | All competitors have SDK/API |
| Complete Node.js SDK (NAPI) | P1 | All competitors have SDK/API |
| MCP Protocol Support | P1 | IronClaw, Claw Code |
| Streaming Responses | P2 | All competitors |
| One-liner Install | P2 | OpenClaw, IronClaw |
| API Documentation (OpenAPI) | P2 | IronClaw 40+ endpoints |
| Web UI Dashboard | P2 | IronClaw gateway UI |
| E2E Tests | P2 | Coverage gap |

---

## 30-Day Sprint Plan Overview

### Week 1 (Days 1-7): Foundation & Memory
| Doc | Task | Deliverable |
|-----|------|-------------|
| [01](01-MEMORY-SYSTEM.md) | Advanced Memory Management | 6-type memory, hybrid search, consolidation |
| [02](02-SKILL-SYSTEM.md) | Skill System (SKILL.md) | Skill format, registry, CLI commands, hot reload |
| [03](03-STREAMING.md) | Streaming Responses | SSE + WebSocket token streaming |

### Week 2 (Days 8-14): Extensibility & Agents
| Doc | Task | Deliverable |
|-----|------|-------------|
| [04](04-SUB-AGENT-SYSTEM.md) | Sub-Agent System | Agent spawning, coordination, isolation |
| [05](05-MCP-SUPPORT.md) | MCP Protocol Support | stdio/HTTP transport, tool discovery |
| [06](06-LEARNING-TRADITIONAL.md) | Traditional Learning | Prompt evolution, skill extraction, conversation insights |

### Week 3 (Days 15-21): SDKs & Social Media
| Doc | Task | Deliverable |
|-----|------|-------------|
| [07](07-PYTHON-SDK.md) | Python SDK (PyO3) | Real bindings, PyPI publish |
| [08](08-NODE-SDK.md) | Node.js SDK (NAPI-rs) | Real bindings, npm publish |
| [09](09-SOCIAL-MEDIA-AGENT.md) | Social Media Manager Agent | Platform integrations, campaign management |

### Week 4 (Days 22-30): Learning, Polish & Launch
| Doc | Task | Deliverable |
|-----|------|-------------|
| [10](10-RL-LEARNING.md) | RL-Based Learning (GRPO) | Reward model, training loop, hot-swap |
| [11](11-INSTALL-AND-CLI.md) | One-Liner Install & CLI Polish | Install scripts, shell completions, doctor command |
| [12](12-WEB-UI-AND-DOCS.md) | Web UI Dashboard & API Docs | Gateway UI, OpenAPI spec, documentation |

---

## Feature Comparison Matrix (After v1.0)

| Feature | OpenPylot | OpenClaw | IronClaw | MetaClaw | Claw Code |
|---------|-----------|----------|----------|----------|-----------|
| Skill System | ✅ | ✅ | ✅ | ✅ | 🟡 |
| Sub-Agents | ✅ | ✅ | 🟡 | ❌ | 🟡 |
| Memory (6-type) | ✅ | 🟡 | ✅ | ✅ | ❌ |
| RL Learning | ✅ | ❌ | 🟡 | ✅ | ❌ |
| Traditional Learning | ✅ | 🟡 | ✅ | ✅ | ❌ |
| MCP Support | ✅ | ❌ | ✅ | ❌ | ✅ |
| Social Media | ✅ | ❌ | ❌ | ❌ | ❌ |
| Python SDK | ✅ | ❌ | 🟡 | ✅ | 🟡 |
| Node.js SDK | ✅ | 🟡 | 🟡 | ❌ | 🟡 |
| Streaming | ✅ | ✅ | ✅ | ❌ | ✅ |
| One-liner Install | ✅ | ✅ | ✅ | ✅ | 🟡 |
| Web UI | ✅ | ✅ | ✅ | ❌ | ❌ |
| 21+ Built-in Tools | ✅ | ✅ | ❌ | ❌ | ✅ |
| Encrypted Secrets | ✅ | ❌ | ✅ | ❌ | ❌ |
| Telegram/WhatsApp | ✅ | ✅ | ✅ | ❌ | ❌ |

**Legend**: ✅ Full | 🟡 Partial | ❌ None

---

## Architecture After v1.0

```
┌─────────────────────────────────────────────────────────────────┐
│                     OpenPylot v1.0 Architecture                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Channels: CLI | REST API | WebSocket | Telegram | Web UI        │
│       ↓                                                          │
│  ┌─────────────────────────────────────────────────────┐        │
│  │              Agent Router / Orchestrator              │        │
│  │  ┌───────────┐  ┌──────────┐  ┌──────────────────┐  │        │
│  │  │ Main Agent│  │Sub-Agent │  │ Social Media Agent│  │        │
│  │  │  (core)   │  │ (spawned)│  │   (specialized)   │  │        │
│  │  └───────────┘  └──────────┘  └──────────────────┘  │        │
│  └─────────────────────────────────────────────────────┘        │
│       ↓                                                          │
│  ┌─────────────────────────────────────────────────────┐        │
│  │                  Tool Registry                        │        │
│  │  Built-in (21+) | Skills (SKILL.md) | MCP | WASM     │        │
│  └─────────────────────────────────────────────────────┘        │
│       ↓                                                          │
│  ┌─────────────────────────────────────────────────────┐        │
│  │              Memory System (6-Type)                    │        │
│  │  Episodic | Semantic | Preference | Project State     │        │
│  │  Working Summary | Procedural Observation             │        │
│  │  ┌──────────────────────────────────────────────┐    │        │
│  │  │ Hybrid Search: FTS5 + Vector (RRF Fusion)    │    │        │
│  │  └──────────────────────────────────────────────┘    │        │
│  └─────────────────────────────────────────────────────┘        │
│       ↓                                                          │
│  ┌─────────────────────────────────────────────────────┐        │
│  │              Learning Engine                          │        │
│  │  Traditional: Prompt Evolution + Skill Extraction     │        │
│  │  RL: GRPO + PRM Scoring + Hot-Weight Swap             │        │
│  └─────────────────────────────────────────────────────┘        │
│       ↓                                                          │
│  ┌─────────────────────────────────────────────────────┐        │
│  │              SDKs & APIs                              │        │
│  │  Python (PyO3) | Node.js (NAPI-rs) | REST | OpenAI   │        │
│  └─────────────────────────────────────────────────────┘        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Document Index

| # | Document | Focus Area | Estimated Days |
|---|----------|-----------|---------------|
| 00 | This file | Master Plan Overview | — |
| 01 | [01-MEMORY-SYSTEM.md](01-MEMORY-SYSTEM.md) | Advanced Memory Management | 3 |
| 02 | [02-SKILL-SYSTEM.md](02-SKILL-SYSTEM.md) | Skill System (SKILL.md format) | 2 |
| 03 | [03-STREAMING.md](03-STREAMING.md) | Streaming Responses | 1 |
| 04 | [04-SUB-AGENT-SYSTEM.md](04-SUB-AGENT-SYSTEM.md) | Sub-Agent Orchestration | 3 |
| 05 | [05-MCP-SUPPORT.md](05-MCP-SUPPORT.md) | Model Context Protocol | 2 |
| 06 | [06-LEARNING-TRADITIONAL.md](06-LEARNING-TRADITIONAL.md) | Traditional Learning Approaches | 2 |
| 07 | [07-PYTHON-SDK.md](07-PYTHON-SDK.md) | Python SDK (PyO3 bindings) | 3 |
| 08 | [08-NODE-SDK.md](08-NODE-SDK.md) | Node.js SDK (NAPI-rs bindings) | 2 |
| 09 | [09-SOCIAL-MEDIA-AGENT.md](09-SOCIAL-MEDIA-AGENT.md) | Social Media Manager Agent | 5 |
| 10 | [10-RL-LEARNING.md](10-RL-LEARNING.md) | Reinforcement Learning (GRPO) | 3 |
| 11 | [11-INSTALL-AND-CLI.md](11-INSTALL-AND-CLI.md) | Installation & CLI Polish | 2 |
| 12 | [12-WEB-UI-AND-DOCS.md](12-WEB-UI-AND-DOCS.md) | Web UI & Documentation | 2 |

**Total: 30 days**

---

## Risk Register

| Risk | Mitigation |
|------|-----------|
| RL training requires GPU cloud backend | Use Tinker-style cloud API; skills_only mode as fallback |
| Social media OAuth tokens expire frequently | Auto-refresh workflow (Postiz pattern) with 3-retry |
| MCP ecosystem fragmentation | Support both MCP + native skills; graceful fallback |
| Python/Node SDK scope creep | Ship core bindings first (chat, memory, tools); iterate |
| 30-day timeline is aggressive | Prioritize P0 items; defer P2 if needed |

---

## How to Use These Docs

Each document (01-12) is a **self-contained implementation guide** designed to be given directly to a coding agent. Each contains:

1. **Objective** — What we're building and why
2. **Reference Implementations** — Exact files to check in competitor repos
3. **Architecture** — Data structures, traits, modules
4. **Implementation Steps** — Ordered, atomic tasks
5. **File Locations** — Exact paths where code should go
6. **Code Examples** — Rust structs, function signatures, SQL schemas
7. **Testing Requirements** — What tests to write
8. **Acceptance Criteria** — How to verify completion
