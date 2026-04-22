# 07 — Python SDK (PyO3 Bindings)

## Objective

Replace the current shell-out Python wrapper with real PyO3 bindings. Publish to PyPI as `openpylot`. Users should be able to `pip install openpylot` and use a native Python API for chat, memory, tools, and agent management.

---

## Current State

- **Directory**: `python/`
- **Build**: maturin + PyO3
- **Current implementation**: Shells out to the `pylot` binary (not real bindings)
- **Files**: `python/src/lib.rs` (PyO3 module), `python/python/` (Python wrapper)
- **pyproject.toml**: Exists but package not published

---

## Reference Implementations

### MetaClaw (Python-native)
- Full Python package on PyPI
- CLI: `metaclaw start/stop/status/config`
- Proxy-based: FastAPI server intercepts LLM calls

### IronClaw (API-based SDK)
- OpenAI-compatible proxy: `/v1/chat/completions`
- RESTful memory/jobs/routines APIs
- Any HTTP client can integrate

---

## Architecture

### What to Expose

```python
import openpylot

# 1. Client connection (connects to running pylot server)
client = openpylot.Client(url="http://localhost:8000", api_key="optional")

# 2. Chat
response = client.chat("What's on my calendar today?")
print(response.text)

# 3. Streaming chat
for event in client.chat_stream("Research AI safety"):
    if event.type == "text_delta":
        print(event.text, end="")

# 4. Memory
results = client.memory.search("user preferences")
client.memory.write("Project uses Python 3.12", type="semantic")
tree = client.memory.tree()

# 5. Tools
tools = client.tools.list()
result = client.tools.execute("web_search", query="latest news")

# 6. Skills
skills = client.skills.list()
client.skills.install("/path/to/SKILL.md")

# 7. Agents (sub-agents)
agent_id = client.agents.spawn(
    name="researcher",
    prompt="Research X",
    tools=["web_search", "web_fetch"]
)
result = client.agents.wait_for(agent_id, timeout=60)

# 8. Embedded mode (no server needed, runs in-process)
pylot = openpylot.Pylot(config_path="~/.pylot/config.toml")
response = pylot.chat("Hello!")
```

### Module Structure

```
python/
├── Cargo.toml              # PyO3 + maturin config
├── pyproject.toml           # Package metadata
├── src/
│   └── lib.rs              # PyO3 module definition
├── python/
│   └── openpylot/
│       ├── __init__.py     # Package entry
│       ├── client.py       # HTTP client (connects to server)
│       ├── embedded.py     # In-process Pylot (via PyO3)
│       ├── memory.py       # Memory API wrapper
│       ├── tools.py        # Tools API wrapper
│       ├── skills.py       # Skills API wrapper
│       ├── agents.py       # Sub-agent API wrapper
│       ├── streaming.py    # SSE streaming client
│       └── types.py        # Response types
└── tests/
    ├── test_client.py
    ├── test_memory.py
    └── test_embedded.py
```

---

## Implementation Steps

### Step 1: Update PyO3 bindings in Rust (Day 1)

**File**: `python/src/lib.rs`

```rust
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Embedded Pylot agent (runs in-process)
#[pyclass]
struct Pylot {
    inner: Arc<tokio::sync::Mutex<crate::Agent>>,
    runtime: tokio::runtime::Runtime,
}

#[pymethods]
impl Pylot {
    #[new]
    fn new(config_path: Option<String>) -> PyResult<Self> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let agent = runtime.block_on(async {
            let config = crate::config::load_config(config_path.as_deref()).await?;
            crate::Agent::new(config).await
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(tokio::sync::Mutex::new(agent)),
            runtime,
        })
    }

    /// Send a chat message and get response
    fn chat(&self, message: &str) -> PyResult<String> {
        let agent = self.inner.clone();
        let msg = message.to_string();
        self.runtime.block_on(async {
            let agent = agent.lock().await;
            agent.handle_message(&msg).await
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Search memory
    fn memory_search(&self, query: &str, limit: Option<usize>) -> PyResult<Vec<PyObject>> {
        // ... delegate to memory store
    }

    /// Write to memory
    fn memory_write(&self, content: &str, memory_type: Option<&str>) -> PyResult<String> {
        // ... delegate to memory store
    }

    /// List available tools
    fn tools_list(&self) -> PyResult<Vec<PyObject>> {
        // ... delegate to tool registry
    }
}

/// OpenPylot Python module
#[pymodule]
fn _openpylot(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Pylot>()?;
    Ok(())
}
```

### Step 2: Implement Python HTTP client (Day 1-2)

**File**: `python/python/openpylot/client.py`

```python
import httpx
import json
from typing import Optional, Iterator
from .types import ChatResponse, MemoryResult, ToolInfo, StreamEvent


class Client:
    """HTTP client that connects to a running OpenPylot server."""

    def __init__(self, url: str = "http://localhost:8000", api_key: Optional[str] = None):
        self.url = url.rstrip("/")
        self.headers = {}
        if api_key:
            self.headers["Authorization"] = f"Bearer {api_key}"
        self._http = httpx.Client(base_url=self.url, headers=self.headers, timeout=120)
        self.memory = MemoryAPI(self._http)
        self.tools = ToolsAPI(self._http)
        self.skills = SkillsAPI(self._http)
        self.agents = AgentsAPI(self._http)

    def chat(self, message: str) -> ChatResponse:
        """Send a message and get a complete response."""
        resp = self._http.post("/api/chat", json={"message": message})
        resp.raise_for_status()
        return ChatResponse(**resp.json())

    def chat_stream(self, message: str) -> Iterator[StreamEvent]:
        """Stream a response token by token via SSE."""
        with httpx.stream(
            "POST",
            f"{self.url}/api/chat/stream",
            json={"message": message},
            headers=self.headers,
            timeout=120,
        ) as resp:
            resp.raise_for_status()
            for line in resp.iter_lines():
                if line.startswith("data: "):
                    data = json.loads(line[6:])
                    event = StreamEvent(**data)
                    yield event
                    if event.type == "message_stop":
                        break

    def close(self):
        self._http.close()

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self.close()
```

**File**: `python/python/openpylot/memory.py`

```python
class MemoryAPI:
    def __init__(self, http: httpx.Client):
        self._http = http

    def search(self, query: str, limit: int = 10, memory_type: str = None) -> list[MemoryResult]:
        params = {"query": query, "limit": limit}
        if memory_type:
            params["type"] = memory_type
        resp = self._http.get("/api/memory/search", params=params)
        resp.raise_for_status()
        return [MemoryResult(**r) for r in resp.json()]

    def write(self, content: str, memory_type: str = "semantic", **kwargs) -> str:
        resp = self._http.post("/api/memory/write", json={
            "content": content, "type": memory_type, **kwargs
        })
        resp.raise_for_status()
        return resp.json()["id"]

    def read(self, id: str) -> MemoryResult:
        resp = self._http.get(f"/api/memory/{id}")
        resp.raise_for_status()
        return MemoryResult(**resp.json())

    def tree(self) -> dict:
        resp = self._http.get("/api/memory/tree")
        resp.raise_for_status()
        return resp.json()
```

### Step 3: Types and package init (Day 2)

**File**: `python/python/openpylot/types.py`

```python
from dataclasses import dataclass
from typing import Optional

@dataclass
class ChatResponse:
    text: str
    tool_calls: list = None
    usage: dict = None

@dataclass
class StreamEvent:
    type: str
    text: str = None
    tool_name: str = None
    tool_id: str = None
    result: str = None
    is_error: bool = False

@dataclass
class MemoryResult:
    id: str
    content: str
    memory_type: str
    score: float = None
    entities: list = None
    topics: list = None

@dataclass
class ToolInfo:
    name: str
    description: str
    parameters: dict = None

@dataclass
class SkillInfo:
    name: str
    description: str
    category: str = None
    version: str = None
```

**File**: `python/python/openpylot/__init__.py`

```python
from ._openpylot import Pylot  # PyO3 embedded mode
from .client import Client
from .types import ChatResponse, StreamEvent, MemoryResult, ToolInfo, SkillInfo

__version__ = "1.0.0"
__all__ = ["Pylot", "Client", "ChatResponse", "StreamEvent", "MemoryResult", "ToolInfo", "SkillInfo"]
```

### Step 4: Update pyproject.toml (Day 2)

**File**: `python/pyproject.toml`

```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "openpylot"
version = "1.0.0"
description = "OpenPylot AI Assistant — Python SDK"
readme = "README.md"
requires-python = ">=3.9"
license = {text = "MIT"}
keywords = ["ai", "assistant", "agent", "llm"]
classifiers = [
    "Development Status :: 4 - Beta",
    "Programming Language :: Python :: 3",
    "Programming Language :: Rust",
]
dependencies = [
    "httpx>=0.25",
]

[project.optional-dependencies]
dev = ["pytest", "pytest-asyncio"]

[tool.maturin]
features = ["pyo3/extension-module"]
python-source = "python"
module-name = "openpylot._openpylot"
```

### Step 5: Write tests (Day 3)

**File**: `python/tests/test_client.py`

```python
import pytest
from openpylot import Client

@pytest.fixture
def client():
    return Client(url="http://localhost:8000")

def test_chat(client):
    response = client.chat("Hello")
    assert response.text
    assert len(response.text) > 0

def test_memory_search(client):
    results = client.memory.search("test query")
    assert isinstance(results, list)

def test_skills_list(client):
    skills = client.skills.list()
    assert isinstance(skills, list)
```

### Step 6: Build and publish (Day 3)

```bash
# Build
cd python
maturin develop  # Local install for testing
maturin build --release  # Build wheel

# Test
pytest tests/

# Publish (when ready)
maturin publish  # Uploads to PyPI
```

---

## Testing

- `test_client_chat` — Send message, get response
- `test_client_stream` — Stream events token by token
- `test_memory_search` — Search and get results
- `test_memory_write_read` — Write then read back
- `test_tools_list` — List available tools
- `test_skills_list` — List skills
- `test_agents_spawn` — Spawn and wait for sub-agent
- `test_embedded_pylot` — PyO3 in-process mode
- `test_error_handling` — Server errors raise Python exceptions

---

## Acceptance Criteria

- [ ] `pip install openpylot` works (from wheel or PyPI)
- [ ] `Client` connects to running server via HTTP
- [ ] `client.chat()` returns response
- [ ] `client.chat_stream()` yields events
- [ ] `client.memory.search/write/read/tree` work
- [ ] `client.tools.list/execute` work
- [ ] `client.skills.list/install` work
- [ ] `client.agents.spawn/wait_for` work
- [ ] `Pylot` embedded mode works (in-process, no server)
- [ ] Type hints and docstrings for all public APIs
- [ ] Tests pass with running server
