# Development Guide

Everything you need to build, test, and contribute to OpenPylot.

---

## Prerequisites

| Tool    | Min. version   | Used for                  |
| ------- | -------------- | ------------------------- |
| Rust    | 1.75           | Core binary               |
| Cargo   | bundled        | Build / test runner       |
| Node.js | 18             | Frontend + Node SDK       |
| npm     | 9              | Frontend + Node SDK       |
| Python  | 3.9            | Python SDK                |
| maturin | 1.x            | Building the Python wheel |
| Docker  | 24+ (optional) | Containerized builds      |

```bash
rustup toolchain install stable
pip install maturin
npm i -g @napi-rs/cli
```

---

## Repository layout

```
.
├── src/                 # Rust crate (binary + library)
│   ├── main.rs          # CLI entry (clap subcommands)
│   ├── lib.rs           # Library re-exports
│   ├── agent.rs         # LLM ↔ tool loop
│   ├── config.rs        # Layered config
│   ├── secrets.rs       # AES-256-GCM vault
│   ├── scheduler.rs     # Tokio cron scheduler
│   ├── api/             # Axum REST + WebSocket
│   ├── llm/             # OpenAI / Anthropic providers
│   ├── tools/           # Built-in tools (calendar, gmail, notes…)
│   ├── skills/          # SKILL.md loader & matcher
│   ├── memory_v2/       # Structured memory
│   ├── smart_memory.rs  # SQLite + embeddings
│   ├── sub_agents/      # Sub-agent orchestrator
│   ├── mcp/             # Model Context Protocol client
│   ├── learning/        # Auto-scorer, prompt evolution
│   ├── social/          # 17 social-media providers
│   ├── marketing/       # Marketing agent
│   ├── streaming/       # WS/SSE token streaming
│   ├── jobs/            # Background job definitions
│   └── webhooks/        # Inbound webhook handlers
├── frontend/            # Next.js 15 web dashboard
├── python/              # Python SDK (PyO3 + maturin)
├── node/                # Node.js SDK (NAPI-RS)
├── agents/              # Bundled agent presets (TOML)
├── skills/              # Bundled skills (SKILL.md)
├── config/default.toml  # Default config
├── docs/                # ← you are here
├── tests/               # rust/, python/, node/
├── Dockerfile
├── docker-compose.yml
├── install.sh
└── Cargo.toml
```

See [ARCHITECTURE.md](./ARCHITECTURE.md) for what each module does and how they interact.

---

## Clone and build

```bash
git clone https://github.com/globalmindventures/OpenPylot.git
cd pylot

# Debug build (fast, ~30s once deps cached)
cargo build

# Release build
cargo build --release
```

Run the freshly built binary without installing:

```bash
./target/debug/pylot --version
./target/debug/pylot serve
```

### Frontend

```bash
cd frontend
npm install
npm run dev          # http://localhost:3000 (dev server)
npm run build        # produces frontend/out/ — picked up by `pylot serve`
```

### Python SDK

```bash
cd python
maturin develop      # builds + installs into the active venv
python -c "import pylot; print(pylot.__version__)"
```

### Node SDK

```bash
cd node
npm install
npm run build        # builds the native NAPI module
npm test
```

---

## Running locally

```bash
# Terminal 1 — backend
RUST_LOG=info cargo run -- serve

# Terminal 2 — frontend (optional)
cd frontend && npm run dev

# Terminal 3 — interactive REPL
cargo run -- chat "hello"
```

For day-to-day work an existing `~/.pylot/` is fine; if you want an isolated workspace:

```bash
export PYLOT_DATA_DIR=$PWD/.pylot-dev
cargo run -- init
```

---

## Testing

```bash
# Rust unit + integration tests
cargo test
cargo test --package pylot --test <test_file>

# Python
cd python && pytest

# Node
cd node && npm test
```

CI runs all three suites on every PR.

### Linting & formatting

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cd frontend && npm run lint && npm run typecheck
```

---

## Adding things

### A new tool

1. Create `src/tools/your_tool.rs` implementing the `Tool` trait.
2. Re-export it from `src/tools/mod.rs`.
3. Register it where the registry is built (search for `ToolRegistry::new`).
4. Add a test in `tests/rust/`.

```rust
use async_trait::async_trait;
use serde_json::{json, Value};
use crate::tools::{Tool, ToolDefinition, ToolResult};

pub struct YourTool;

#[async_trait]
impl Tool for YourTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "your_tool".into(),
            description: "What it does".into(),
            parameters: json!({
                "type": "object",
                "properties": { "input": { "type": "string" } },
                "required": ["input"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> anyhow::Result<ToolResult> {
        let input = params["input"].as_str().unwrap_or_default();
        Ok(ToolResult::ok(format!("got: {input}")))
    }
}
```

### A new skill (no Rust needed)

Drop a `SKILL.md` file under `skills/<category>/<name>/`. See [PLUGINS.md](./PLUGINS.md).

### A new sub-agent preset (no Rust needed)

Drop a `<name>.toml` under `agents/`. See [PLUGINS.md](./PLUGINS.md).

### A new API endpoint

1. Add a handler in `src/api/handlers.rs`.
2. Register the route in `src/api/mod.rs`.
3. Document it in [API.md](./API.md).

### A new LLM provider

1. Create `src/llm/<provider>.rs` implementing the `LlmProvider` trait.
2. Wire it into the provider selector in `src/llm/mod.rs`.
3. Add config keys in `src/config.rs`.

---

## Common cargo recipes

```bash
cargo run -- doctor                  # diagnostics
cargo run -- chat "hi" -- --debug    # one-shot with verbose
cargo build --release && \
  cp target/release/pylot ~/.local/bin/
RUST_LOG=pylot=debug cargo run -- serve
```

---

## Commit & PR conventions

- Follow conventional-commits-ish prefixes: `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.
- Branch off `main` (or the active `feat/*` branch as directed in the issue).
- Keep PRs focused; update [CHANGELOG.md](../CHANGELOG.md) under **Unreleased**.
- A PR is mergeable when: tests green, clippy clean, docs updated.

See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full contributor guide.

---

## Releasing

1. Bump version in `Cargo.toml`, `python/pyproject.toml`, `node/package.json`, `frontend/package.json`.
2. Update `CHANGELOG.md` (move _Unreleased_ → new version).
3. Tag: `git tag v0.x.y && git push --tags`.
4. CI builds binaries, Python wheels, and the npm package and attaches them to the GitHub release.
5. Update the Homebrew formula at `Formula/pylot.rb`.

---

## Debugging tips

| Want to…                    | Try                                                 |
| --------------------------- | --------------------------------------------------- |
| Trace the LLM↔tool loop     | `RUST_LOG=pylot::agent=trace cargo run -- chat "…"` |
| See HTTP requests           | `RUST_LOG=reqwest=debug …`                          |
| Inspect the SQLite memory   | `sqlite3 ~/.pylot/data/smart_memory.db`             |
| Reset everything            | `rm -rf ~/.pylot && pylot init`                     |
| Replay a saved conversation | open `~/.pylot/data/conversations/<id>.json`        |
