# OpenPylot — Python SDK

[![PyPI](https://img.shields.io/pypi/v/openpylot)](https://pypi.org/project/openpylot/)

Python bindings for **[OpenPylot](https://github.com/gmvofficial/OpenPylot)** — a
Rust-powered personal AI assistant. Drive the agent from Python: chat, manage
memory, run skills, and register your own tools, all backed by the same fast Rust
core that powers the `pylot` CLI.

The package is a compiled extension (built with [PyO3](https://pyo3.rs) +
[maturin](https://www.maturin.rs)) — `agent.chat()` and the memory/skills APIs run
**in-process**, no server required.

---

## Requirements

- **Python 3.9+**
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
  > your encrypted config.

---

## Installation

```bash
pip install openpylot
```

---

## Quick start

### 1. Configure once

Run the setup wizard (choose an LLM provider, paste an API key). This creates your
encrypted config at `~/.pylot/`:

```bash
pylot init
```

…or from Python:

```python
from pylot import PylotAgent

PylotAgent.init()   # launches the terminal wizard
```

### 2. Chat

```python
from pylot import PylotAgent

agent = PylotAgent.from_config("~/.pylot/secrets.enc")
reply = agent.chat("What meetings do I have today?")
print(reply)
```

---

## Configure in code (headless / CI)

Skip the wizard and pass credentials directly — handy for servers and pipelines:

```python
from pylot import PylotAgent, Config

config = Config(
    llm_provider="anthropic",
    llm_model="claude-sonnet-4-20250514",
    anthropic_api_key="sk-ant-...",
    # optional integrations:
    telegram_bot_token="...",
)
agent = PylotAgent(config)
print(agent.chat("Schedule a meeting with John tomorrow at 3pm"))
```

`Config` fields: `llm_provider`, `llm_model`, `openai_api_key`,
`anthropic_api_key`, `google_credentials_file`, `telegram_bot_token`,
`telegram_chat_id`.

---

## Register custom tools

Give the agent new abilities by exposing Python functions as tools:

```python
def search_web(query: str) -> str:
    """Your implementation."""
    return "results..."

agent.register_tool(
    name="search_web",
    schema='{"type":"object","properties":{"query":{"type":"string"}}}',
    callback=search_web,
)
```

---

## Also available

Beyond `PylotAgent`, the package exposes the assistant's subsystems:

| Class | Purpose |
|-------|---------|
| `PylotMemory` | Store and search the persistent memory database |
| `PylotSkills` | List installed skills |
| `PylotLearning` | Inspect learned rules and submit feedback |

```python
from pylot import PylotMemory

mem = PylotMemory()                      # defaults to ~/.pylot/data
mem.remember("User prefers dark mode", "preference")
print(mem.search("dark mode", 5))
```

---

## CLI

Installing also puts the `pylot` command on your `PATH`:

```bash
pylot init          # interactive setup
pylot chat "Hi"     # one-shot question
pylot doctor        # diagnose configuration
pylot serve         # background daemon + web dashboard
```

Run `pylot --help` for the full command list.

---

## Development

```bash
pip install maturin
cd python
maturin develop            # build + install locally
pip install -e ".[dev]"
pytest
```

---

## Troubleshooting

**`pylot binary not found`** — install the native binary (see
[Requirements](#requirements)), or verify it's on your `PATH` with `pylot --version`.

**`No API key configured`** — run `pylot init` (or pass a `Config` with your key).

---

## Links

- Main project & full docs: <https://github.com/gmvofficial/OpenPylot>
- Rust crate: <https://crates.io/crates/openpylot>
- npm package: <https://www.npmjs.com/package/openpylot>

## License

Apache-2.0
