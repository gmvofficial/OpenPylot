# Contributing to OpenPylot

Thank you for your interest in contributing to OpenPylot! This guide will help you get started.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Coding Standards](#coding-standards)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)

## Getting Started

1. **Fork** the repository on GitHub
2. **Clone** your fork locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/pylot.git
   cd pylot
   ```
3. **Create a branch** for your work:
   ```bash
   git checkout -b feature/your-feature-name
   ```

## Development Setup

### Prerequisites

- **Rust 1.75+** — Install via [rustup](https://rustup.rs/)
- **Git** — Version control
- **pre-commit** — Git hooks (optional but recommended)

### Quick Setup

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build
git clone https://github.com/globalmindventures/OpenPylot.git
cd pylot
cargo build

# Run tests
cargo test

# Install pre-commit hooks (optional)
pip install pre-commit  # or: brew install pre-commit
pre-commit install
```

### Environment Setup

1. Copy the example env file:
   ```bash
   cp .env.example .env
   ```
2. Fill in your API keys in `.env`, or use the interactive setup:
   ```bash
   cargo run -- init
   ```

## Project Structure

```
src/
├── main.rs              # CLI entry point (clap)
├── lib.rs               # Library crate (re-exports all modules)
├── agent.rs             # Agent loop: LLM ↔ tool calls
├── config.rs            # Layered config (env > vault > TOML > defaults)
├── context.rs           # Conversation context management
├── context_builder.rs   # Context building utilities
├── document_chunker.rs  # Document chunking for knowledge base
├── memory.rs            # Persistent memory store (JSON)
├── smart_memory.rs      # SQLite + embeddings semantic memory
├── secrets.rs           # AES-256-GCM encrypted vault
├── traits.rs            # Core traits
├── init.rs              # Setup wizard, doctor, status
├── terminal.rs          # Interactive REPL
├── scheduler.rs         # Tokio cron scheduler
├── oauth.rs             # Browser-based OAuth 2.0 flows
├── telegram_bot.rs      # Telegram long-polling bot
├── api/                 # Axum REST API + WebSocket handlers
├── llm/                 # LLM provider trait + OpenAI, Anthropic
├── tools/               # Tool registry + built-in tools
├── webhooks/            # Webhook endpoint handlers
├── jobs/                # Background job definitions
├── skills/              # Skill system (SKILL.md loader, matcher)
├── memory_v2/           # Memory v2 (structured memory types)
├── streaming/           # Token streaming (WebSocket, SSE)
├── sub_agents/          # Sub-agent orchestration
├── mcp/                 # Model Context Protocol client
├── learning/            # Auto-scorer, prompt evolution, skill evolver
├── social/              # Social media manager (17 providers)
└── marketing/           # Marketing agent (campaigns, content)

frontend/                # Next.js web dashboard
python/                  # Python SDK (PyO3)
node/                    # Node.js SDK (NAPI-RS)
config/default.toml      # Default configuration
tests/                   # Test suites
docs/                    # Documentation
```

## Coding Standards

### Rust Style

- **Format**: Always run `cargo fmt` before committing
- **Lint**: Code must pass `cargo clippy -- -D warnings`
- **Edition**: Rust 2021
- **Error handling**: Use `anyhow::Result` for application errors, `thiserror` for library errors
- **Async**: Use `tokio` with `async/await`

### Naming Conventions

| Item | Convention | Example |
|------|-----------|---------|
| Modules | `snake_case` | `rsvp_monitor` |
| Structs/Enums | `PascalCase` | `AppConfig`, `RsvpState` |
| Functions | `snake_case` | `check_changes()` |
| Constants | `SCREAMING_SNAKE_CASE` | `SALT_LEN` |
| Type parameters | Single uppercase letter | `T`, `E` |

### Code Organization

- Keep modules focused and cohesive
- Use `pub` sparingly — prefer minimal public API
- Add doc comments (`///`) on all public items
- Group imports: std, external crates, internal modules

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add WhatsApp notification support
fix: resolve calendar OAuth token refresh
docs: update installation guide
test: add RSVP monitor integration tests
refactor: extract encryption helpers to module
chore: update dependencies
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run a specific test file
cargo test --test rsvp_test

# Run tests with output
cargo test -- --nocapture

# Run only unit tests (no integration tests)
cargo test --lib
```

### Writing Tests

- **Unit tests**: Place in `#[cfg(test)] mod tests { ... }` at bottom of source file
- **Integration tests**: Place in `tests/` directory, import via `pylot::`
- **Test naming**: `test_<what>_<scenario>` e.g., `test_rsvp_detect_change`
- **Assertions**: Use descriptive messages: `assert!(x, "explanation")`

Example unit test:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_function_returns_expected() {
        let result = my_function("input");
        assert_eq!(result, "expected", "my_function should return expected for input");
    }
}
```

### Test Coverage

All new features must include tests. Aim for:
- Happy path tests
- Edge cases (empty input, missing data)
- Error conditions
- State persistence (save/load roundtrips)

## Pull Request Process

1. **Update your branch** with the latest main:
   ```bash
   git fetch origin
   git rebase origin/main
   ```

2. **Ensure all checks pass**:
   ```bash
   cargo fmt -- --check
   cargo clippy -- -D warnings
   cargo test
   ```

3. **Write a clear PR description**:
   - What the change does
   - Why it's needed
   - How to test it
   - Related issue numbers

4. **Keep PRs focused** — one feature or fix per PR

5. **Respond to review feedback** promptly

### PR Requirements

- [ ] All tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] New features have tests
- [ ] Public APIs have doc comments
- [ ] Commit messages follow conventional commits

## Issue Guidelines

### Bug Reports

Include:
- OpenPylot version (`pylot --version`)
- OS and architecture
- Steps to reproduce
- Expected vs. actual behavior
- Error messages or logs

### Feature Requests

Include:
- Description of the feature
- Use case / motivation
- Proposed implementation (if any)

## Architecture Notes

### Configuration Priority

1. Environment variables (highest)
2. Encrypted secrets vault (`~/.pylot/secrets.enc`)
3. TOML config (`config/default.toml` or `~/.pylot/config.toml`)
4. Built-in defaults (lowest)

### Security

- **Never commit secrets** — API keys go in `.env` or the encrypted vault
- **Secrets vault** uses AES-256-GCM with Argon2id key derivation
- **Machine-bound** — vault is encrypted with a machine-specific ID

## Getting Help

- Open an issue for bugs or feature requests
- Check existing issues before creating new ones
- Be respectful and constructive in all interactions

---

Thank you for contributing to OpenPylot!
