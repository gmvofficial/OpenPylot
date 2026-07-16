# OpenPylot — Node.js SDK

[![npm](https://img.shields.io/npm/v/openpylot)](https://www.npmjs.com/package/openpylot)

Node.js / TypeScript bindings for **[OpenPylot](https://github.com/gmvofficial/OpenPylot)** —
a Rust-powered personal AI assistant. Drive the agent from JavaScript: chat,
manage memory, run skills, all backed by the same fast Rust core that powers the
`pylot` CLI.

The package is a native addon (built with [NAPI-RS](https://napi.rs)) — `agent.chat()`
and the memory/skills APIs run **in-process**, no server required. Prebuilt binaries
ship for macOS and Linux (x64 + arm64); npm installs the right one automatically.

---

## Requirements

- **Node.js 16+**
- **The `pylot` native binary** on your `PATH` — used for the interactive setup
  wizard and diagnostics (`init`, `doctor`, `status`) and the `pylot` command.
  Install it with either:

  ```bash
  cargo install openpylot
  # or the one-line installer (macOS / Linux):
  curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash
  ```

  > Not sure if it's installed? Run `pylot --version`. The programmatic
  > `agent.chat()` API works without it, but first-time setup needs it to create
  > your encrypted config. You can also point the wrapper at a specific binary with
  > the `PYLOT_BINARY` environment variable.

---

## Installation

```bash
npm install openpylot
```

---

## Quick start

### 1. Configure once

Run the setup wizard (choose an LLM provider, paste an API key). This creates your
encrypted config at `~/.pylot/`:

```bash
pylot init
```

### 2. Chat

```typescript
import { PylotAgent } from 'openpylot';

const agent = await PylotAgent.fromConfig('~/.pylot/secrets.enc');
const reply = await agent.chat('What meetings do I have today?');
console.log(reply);
```

---

## Configure in code (headless / CI)

Skip the wizard and pass credentials directly — handy for servers and pipelines:

```typescript
import { PylotAgent } from 'openpylot';

const agent = PylotAgent.fromOptions({
  llmProvider: 'anthropic',
  llmModel: 'claude-sonnet-4-20250514',
  anthropicApiKey: process.env.ANTHROPIC_API_KEY,
  // optional integrations:
  telegramBotToken: process.env.TELEGRAM_BOT_TOKEN,
});

console.log(await agent.chat('Schedule a meeting with John tomorrow at 3pm'));
```

`Config` fields: `llmProvider`, `llmModel`, `openaiApiKey`, `anthropicApiKey`,
`googleCredentialsFile`, `telegramBotToken`, `telegramChatId`.

---

## Also available

Beyond `PylotAgent`, the package exposes the assistant's subsystems:

| Class | Purpose |
|-------|---------|
| `PylotMemory` | Store and search the persistent memory database |
| `PylotSkills` | List installed skills |
| `PylotLearning` | Inspect learned rules and submit feedback |

```typescript
import { PylotMemory } from 'openpylot';

const mem = new PylotMemory();              // defaults to ~/.pylot/data
mem.remember('User prefers dark mode', 'preference');
console.log(mem.search('dark mode', 5));
```

Full type declarations are in [index.d.ts](./index.d.ts).

---

## CLI

Installing globally puts the `pylot` command on your `PATH`, delegating to the
native binary:

```bash
npm install -g openpylot

pylot init          # interactive setup
pylot chat "Hi"     # one-shot question
pylot doctor        # diagnose configuration
pylot serve         # background daemon + web dashboard
```

Run `pylot --help` for the full command list.

---

## Development

```bash
cd node
npm install
npm run build       # produces openpylot.<platform>.node
npm test
```

---

## Troubleshooting

**`native pylot binary was not found`** — install the native binary (see
[Requirements](#requirements)), verify it with `pylot --version`, or set
`PYLOT_BINARY=/path/to/pylot`.

**`No API key configured`** — run `pylot init` (or use `fromOptions` with your key).

---

## Links

- Main project & full docs: <https://github.com/gmvofficial/OpenPylot>
- Rust crate: <https://crates.io/crates/openpylot>
- PyPI package: <https://pypi.org/project/openpylot/>

## License

Apache-2.0
