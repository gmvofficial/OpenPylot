# Getting Started

A 5-minute walkthrough to get OpenPylot running locally and have your first conversation with it.

For exhaustive install methods (Homebrew, Docker, building from source, system service), see [INSTALLATION.md](./INSTALLATION.md).

---

## 1. Prerequisites

- macOS or Linux (Windows via WSL2)
- An OpenAI **or** Anthropic API key
- (Optional) Rust 1.75+ if building from source

## 2. Install

```bash
curl -fsSL https://raw.githubusercontent.com/globalmindventures/OpenPylot/main/install.sh | bash
```

This installs the `pylot` binary into `~/.pylot/bin/` and adds it to your `PATH`.

Verify:

```bash
pylot --version
```

## 3. First-time setup

```bash
pylot init
```

The interactive wizard will ask for:

1. **LLM provider** — `openai` or `anthropic`
2. **API key** — stored encrypted in `~/.pylot/secrets.enc`
3. **Agent name & persona** — how the assistant introduces itself
4. **Integrations** — optional: Google Calendar, Gmail, Telegram, WhatsApp

To skip integrations and just configure the LLM:

```bash
pylot init --only llm
```

## 4. Verify

```bash
pylot doctor
```

Expected output: green checks for config file, vault, LLM key, and reachable provider.

## 5. Talk to it

Interactive chat:

```bash
pylot
```

```
You> What can you do?
Pylot: I can manage your notes, reminders, calendar, email, …
```

One-shot:

```bash
pylot chat "Remind me to review PRs at 5pm"
```

## 6. (Optional) Start the backend service

The API server (used by the web dashboard and SDKs) runs on port `3001`:

```bash
pylot serve
```

In another terminal, check it's up:

```bash
curl http://localhost:3001/api/status
```

## 7. (Optional) Start the web dashboard

```bash
cd frontend
npm install
npm run dev          # http://localhost:3000
```

---

## What next?

- **Connect services** → [SOCIAL-PLATFORMS.md](./SOCIAL-PLATFORMS.md) and [INSTALLATION.md](./INSTALLATION.md#google-calendar)
- **Customize** → [CONFIGURATION.md](./CONFIGURATION.md)
- **Integrate from code** → [API.md](./API.md)
- **Deploy** → [DEPLOYMENT.md](./DEPLOYMENT.md)

## Common first-run issues

| Problem | Fix |
|---------|-----|
| `No LLM API key configured` | Re-run `pylot init` or export `OPENAI_API_KEY` |
| `Port 3001 already in use` | `PYLOT_API_PORT=3010 pylot serve` |
| Google OAuth doesn't open browser | Visit the URL printed in the terminal manually |
| `pylot: command not found` | Add `~/.pylot/bin` to your `PATH` and restart the shell |

Run `pylot doctor` any time — it diagnoses most setup problems automatically.
