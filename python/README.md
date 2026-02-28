# GMV Agent — Python Bindings

Python bindings for [GMV Agent](https://github.com/GMV-AI/gmv-agent), a Rust-powered personal AI assistant.

## Installation

```bash
pip install gmv-agent
```

> **Note:** The Rust binary must also be installed. The Python package wraps
> the native binary via PyO3 and also provides a CLI shim.

## Quick Start

### Interactive Setup

```python
from gmv_agent import GMVAgent

GMVAgent.init()  # Launches the terminal wizard
```

### Chat

```python
from gmv_agent import GMVAgent

agent = GMVAgent.from_config("~/.gmv-agent/secrets.enc")
response = agent.chat("What meetings do I have today?")
print(response)
```

### Programmatic / CI Setup

```python
from gmv_agent import GMVAgent, Config

config = Config(
    llm_provider="openai",
    llm_model="gpt-4o",
    openai_api_key="sk-...",
    telegram_bot_token="...",
)
agent = GMVAgent(config)
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

Once installed, `gmv-agent` is available on your PATH:

```bash
gmv-agent init          # Interactive setup
gmv-agent chat "Hi"     # One-shot chat
gmv-agent serve         # Background daemon
gmv-agent doctor        # Diagnostics
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
