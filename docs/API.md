# API Reference

The OpenPylot backend exposes a REST + WebSocket API used by the web dashboard, the SDKs, and any third-party clients. Start the server with:

```bash
pylot serve            # listens on 0.0.0.0:3001 by default
```

- **Base URL:** `http://localhost:3001`
- **API prefix:** `/api`
- **WebSocket prefix:** `/ws`
- **Static uploads:** `/uploads/*`
- **Content type:** `application/json` (unless otherwise noted)
- **CORS:** open by default — restrict via reverse proxy in production (see [DEPLOYMENT.md](./DEPLOYMENT.md))

> 🛈 The API currently has no built-in auth layer. Treat it as a local/private service and put it behind a reverse proxy with auth for any non-localhost deployment.

---

## Conventions

- All endpoints return JSON unless documented otherwise.
- Errors return:
  ```json
  { "error": "human-readable message" }
  ```
  with a non-2xx status code.
- IDs are UUIDs (v4) or stable string slugs depending on the resource.
- Timestamps are RFC 3339 (UTC) strings.

---

## Status

### `GET /api/status`

Returns agent status, model in use, and connected integrations.

```json
{
  "agent_name": "Pylot",
  "llm_provider": "openai",
  "model": "gpt-4o",
  "integrations": ["google_calendar", "gmail", "telegram"],
  "uptime_seconds": 1342
}
```

---

## Chat

### `POST /api/chat`

Send a single message and receive the agent's full reply.

**Request**

```json
{
  "message": "What meetings do I have today?",
  "conversation_id": "optional-uuid"
}
```

**Response**

```json
{
  "conversation_id": "8f3a…",
  "reply": "You have one meeting today — Team Standup at 10:00 AM.",
  "tool_calls": [{ "name": "list_calendar_events", "ok": true }]
}
```

### `POST /api/chat/stream`

Same payload as `POST /api/chat`. Returns **Server-Sent Events** (`text/event-stream`) with incremental tokens:

```
event: token
data: {"text": "You "}

event: token
data: {"text": "have "}

event: done
data: {"conversation_id": "8f3a…"}
```

### `WS /ws/chat`

Bidirectional WebSocket for real-time chat. Send and receive JSON frames:

```json
// client → server
{ "type": "message", "text": "Set a reminder for 5pm", "conversation_id": "…" }
```

```json
// server → client (streamed)
{ "type": "token", "text": "OK" }
{ "type": "tool_call", "name": "set_reminder" }
{ "type": "done", "conversation_id": "…" }
```

---

## Conversations

| Method   | Endpoint                  | Description               |
| -------- | ------------------------- | ------------------------- |
| `GET`    | `/api/conversations`      | List recent conversations |
| `GET`    | `/api/conversations/{id}` | Get full message history  |
| `DELETE` | `/api/conversations/{id}` | Delete a conversation     |

---

## Tools

| Method | Endpoint     | Description                                                       |
| ------ | ------------ | ----------------------------------------------------------------- |
| `GET`  | `/api/tools` | List all registered tools (name, description, JSON-schema params) |

---

## Integrations

Generic CRUD for third-party services (Google, Telegram, WhatsApp, GitHub, Slack, …).

| Method   | Endpoint                              | Description                             |
| -------- | ------------------------------------- | --------------------------------------- |
| `GET`    | `/api/integrations`                   | List services + connection state        |
| `POST`   | `/api/integrations/{service}/connect` | Start connect flow / submit credentials |
| `DELETE` | `/api/integrations/{service}`         | Disconnect & wipe credentials           |
| `POST`   | `/api/integrations/{service}/test`    | Live connectivity test                  |

`{service}` is one of: `google_calendar`, `gmail`, `telegram`, `whatsapp`, `github`, `slack`.

---

## Setup wizard

Used by the web dashboard's onboarding flow.

| Method | Endpoint                  | Body                                      |
| ------ | ------------------------- | ----------------------------------------- |
| `GET`  | `/api/setup/status`       | —                                         |
| `POST` | `/api/setup/llm`          | `{ "provider", "api_key", "model" }`      |
| `POST` | `/api/setup/identity`     | `{ "name", "persona" }`                   |
| `POST` | `/api/setup/telegram`     | `{ "bot_token" }`                         |
| `POST` | `/api/setup/whatsapp`     | `{ "account_sid", "auth_token", "from" }` |
| `POST` | `/api/setup/google`       | `{ "client_id", "client_secret" }`        |
| `POST` | `/api/setup/validate-key` | `{ "provider", "api_key" }`               |

---

## Settings

| Method  | Endpoint        | Description                    |
| ------- | --------------- | ------------------------------ |
| `GET`   | `/api/settings` | Get the resolved configuration |
| `PATCH` | `/api/settings` | Update fields (partial)        |

---

## Memory

| Method   | Endpoint                | Description                                             |
| -------- | ----------------------- | ------------------------------------------------------- |
| `GET`    | `/api/memory`           | List stored memory facts                                |
| `PATCH`  | `/api/memory/{id}`      | Edit a fact                                             |
| `DELETE` | `/api/memory/{id}`      | Delete a fact                                           |
| `POST`   | `/api/memory/v2/search` | Semantic search over memory v2 (`{ "query", "limit" }`) |
| `GET`    | `/api/memory/v2/units`  | List structured memory units                            |

---

## Knowledge base

| Method   | Endpoint                                    | Description                                      |
| -------- | ------------------------------------------- | ------------------------------------------------ |
| `GET`    | `/api/knowledge/collections`                | List collections                                 |
| `POST`   | `/api/knowledge/collections`                | Create collection `{ "name" }`                   |
| `DELETE` | `/api/knowledge/collections/{id}`           | Delete collection                                |
| `GET`    | `/api/knowledge/collections/{id}/documents` | List documents in collection                     |
| `GET`    | `/api/knowledge/documents`                  | List all documents                               |
| `POST`   | `/api/knowledge/documents`                  | Upload (multipart form: `file`, `collection_id`) |
| `POST`   | `/api/knowledge/documents/upload-stream`    | Streamed upload for large files                  |
| `DELETE` | `/api/knowledge/documents/{id}`             | Delete document                                  |
| `POST`   | `/api/knowledge/search`                     | `{ "query", "limit", "collection_id?" }`         |
| `POST`   | `/api/knowledge/extract-document`           | Preview text extraction (multipart, no save)     |

Max upload size: **100 MB**.

---

## Jobs (scheduler)

| Method  | Endpoint             | Description                                          |
| ------- | -------------------- | ---------------------------------------------------- |
| `GET`   | `/api/jobs`          | List scheduled jobs                                  |
| `PATCH` | `/api/jobs/{id}`     | Update job `{ "enabled": bool, "schedule": "cron" }` |
| `POST`  | `/api/jobs/{id}/run` | Trigger a job immediately                            |

Built-in jobs: `reminder_check`, `rsvp_monitor`, `meeting_reminder`, `calendar_sync`, `token_refresh`, `daily_briefing`, `email_digest`.

---

## Skills

| Method   | Endpoint                    | Description                   |
| -------- | --------------------------- | ----------------------------- |
| `GET`    | `/api/skills`               | List loaded skills            |
| `GET`    | `/api/skills/status`        | Skill system status           |
| `POST`   | `/api/skills/update`        | Create / update a skill       |
| `DELETE` | `/api/skills/delete/{name}` | Delete a user skill           |
| `POST`   | `/api/skills/scan`          | Re-scan skill directories     |
| `GET`    | `/api/skills/detail/{name}` | Get one skill's full SKILL.md |

See [PLUGINS.md](./PLUGINS.md) for the SKILL.md format.

---

## Sub-agents

| Method   | Endpoint                     | Description                                     |
| -------- | ---------------------------- | ----------------------------------------------- |
| `GET`    | `/api/agents`                | List active & past sub-agents                   |
| `POST`   | `/api/agents`                | Spawn `{ "name", "task", "preset?", "model?" }` |
| `GET`    | `/api/agents/presets`        | List available agent presets                    |
| `GET`    | `/api/agents/presets/{name}` | Get preset details                              |
| `GET`    | `/api/agents/{id}`           | Status of one sub-agent                         |
| `DELETE` | `/api/agents/{id}`           | Cancel a running sub-agent                      |
| `GET`    | `/api/agents/{id}/runs`      | List run history                                |
| `DELETE` | `/api/agents/{id}/runs`      | Clear run history                               |
| `DELETE` | `/api/agents/{id}/permanent` | Permanently remove sub-agent                    |

See [AGENTS.md](./AGENTS.md).

---

## Social media

| Method   | Endpoint                            | Description                                                  |
| -------- | ----------------------------------- | ------------------------------------------------------------ |
| `GET`    | `/api/social/platforms`             | List supported platforms + connection state                  |
| `POST`   | `/api/social/connect/{platform}`    | Submit credentials for a platform                            |
| `POST`   | `/api/social/disconnect/{platform}` | Disconnect a platform                                        |
| `GET`    | `/api/social/posts`                 | List drafts & published posts                                |
| `POST`   | `/api/social/posts`                 | Create a draft `{ "platforms", "text", "media?" }`           |
| `DELETE` | `/api/social/posts/{id}`            | Delete a post                                                |
| `POST`   | `/api/social/posts/{id}/publish`    | Publish to selected platforms                                |
| `POST`   | `/api/social/improve-post`          | LLM-rewrite a draft `{ "text", "tone?" }`                    |
| `POST`   | `/api/social/upload`                | Upload media (multipart `file`) → returns `/uploads/...` URL |
| `GET`    | `/api/social/campaigns`             | List marketing campaigns                                     |
| `POST`   | `/api/social/campaigns`             | Create a campaign                                            |

Platform IDs and per-platform credential fields are documented in [SOCIAL-PLATFORMS.md](./SOCIAL-PLATFORMS.md).

---

## Learning

| Method | Endpoint                 | Description                                        |
| ------ | ------------------------ | -------------------------------------------------- |
| `GET`  | `/api/learning/rules`    | List rules / heuristics learned from feedback      |
| `POST` | `/api/learning/feedback` | Submit `{ "conversation_id", "rating", "notes?" }` |

---

## MCP (Model Context Protocol)

| Method | Endpoint           | Description                                  |
| ------ | ------------------ | -------------------------------------------- |
| `GET`  | `/api/mcp/servers` | List configured MCP servers and their status |
| `GET`  | `/api/mcp/tools`   | List tools exposed by MCP servers            |

---

## Logs

### `GET /api/logs`

Query parameters:

| Param   | Type    | Default | Description                                       |
| ------- | ------- | ------- | ------------------------------------------------- |
| `level` | string  | `info`  | `trace` \| `debug` \| `info` \| `warn` \| `error` |
| `limit` | integer | `200`   | Max lines                                         |

```json
{
  "lines": [
    {
      "ts": "2026-05-20T10:14:02Z",
      "level": "info",
      "target": "agent",
      "message": "tool: list_calendar_events"
    }
  ]
}
```

---

## Notifications (WebSocket)

### `WS /ws/notifications`

Server-pushed events: scheduled job results, reminder fires, sub-agent state changes.

```json
{ "type": "reminder", "id": "…", "text": "Review PRs", "at": "2026-05-20T17:00:00Z" }
{ "type": "agent_status", "id": "…", "name": "researcher", "state": "completed" }
{ "type": "job", "name": "daily_briefing", "ok": true }
```

---

## Static files

`GET /uploads/{filename}` — user-uploaded media used by social posts. Stored under `~/.pylot/data/uploads/`.

---

## Quick reference: route map

```
/api
├── /status
├── /chat                 POST
├── /chat/stream          POST (SSE)
├── /conversations        GET / {id} GET DELETE
├── /tools                GET
├── /integrations         GET / {service}/connect POST / {service} DELETE / {service}/test POST
├── /setup                /status, /llm, /identity, /telegram, /whatsapp, /google, /validate-key
├── /settings             GET PATCH
├── /memory               GET / {id} PATCH DELETE
│   └── /v2               /search POST, /units GET
├── /knowledge            /collections, /documents, /search, /extract-document
├── /jobs                 GET / {id} PATCH / {id}/run POST
├── /skills               /, /status, /update, /delete/{name}, /scan, /detail/{name}
├── /agents               GET POST / {id} GET DELETE / {id}/runs / presets
├── /social               /platforms, /connect/{p}, /posts, /campaigns, /upload, /improve-post
├── /learning             /rules, /feedback
├── /mcp                  /servers, /tools
└── /logs                 GET

/ws
├── /chat                 (bidirectional)
└── /notifications        (server push)

/uploads/*                static
```
