# OpenPylot — Python Bindings

Python bindings for [OpenPylot](https://github.com/globalmindventures/OpenPylot), a Rust-powered personal AI assistant.

## Installation

```bash
pip install openpylot
```

> **Note:** The Rust binary must also be installed. The Python package wraps
> the native binary via PyO3 and also provides a CLI shim.

## Quick Start

### Interactive Setup

```python
from pylot import PylotAgent

PylotAgent.init()  # Launches the terminal wizard
```

### Chat

```python
from pylot import PylotAgent

agent = PylotAgent.from_config("~/.pylot/secrets.enc")
response = agent.chat("What meetings do I have today?")
print(response)
```

### Programmatic / CI Setup

```python
from pylot import PylotAgent, Config

config = Config(
    llm_provider="openai",
    llm_model="gpt-4o",
    openai_api_key="sk-...",
    telegram_bot_token="...",
)
agent = PylotAgent(config)
response = agent.chat("Schedule a meeting with John tomorrow at 3pm")
```

### Custom Tools

```python
def search_web(query: str) -> str:
    # your implementation
    return "results..."

agent.register_tool(
    name="search_web",
    schema='{"type":"object","properties":{"query":{"type":"string"}}}',
    callback=search_web,
)
```

## CLI

Once installed, `pylot` is available on your PATH:

```bash
pylot init          # Interactive setup
pylot chat "Hi"     # One-shot chat
pylot serve         # Background daemon
pylot doctor        # Diagnostics
```

## Development

```bash
# Install maturin
pip install maturin

# Build and install in development mode
cd python
maturin develop

# Run tests
pip install -e ".[dev]"
pytest
```

## License

MIT
