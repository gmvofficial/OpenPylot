# GMV Agent — Installation Guide

Complete guide for installing, configuring, and running GMV Agent.

## Table of Contents

- [Quick Install](#quick-install)
- [Manual Installation](#manual-installation)
- [Homebrew](#homebrew)
- [Docker](#docker)
- [Python Package](#python-package)
- [Node.js Package](#nodejs-package)
- [Build from Source](#build-from-source)
- [Configuration](#configuration)
- [First Run](#first-run)
- [Background Service](#background-service)
- [Upgrading](#upgrading)
- [Uninstalling](#uninstalling)
- [Troubleshooting](#troubleshooting)

---

## Quick Install

One-line installer (macOS / Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/GMV-AI/gmv-agent/main/install.sh | bash
```

This will:
1. Detect your platform (macOS/Linux, x86_64/aarch64)
2. Download the latest release binary
3. Install to `~/.gmv-agent/bin/`
4. Add to your PATH
5. Launch the interactive setup wizard

### Installer Options

| Variable | Description | Default |
|----------|-------------|---------|
| `GMV_VERSION` | Specific version to install | `latest` |
| `GMV_PREFIX` | Installation directory | `~/.gmv-agent` |
| `GMV_NO_INIT` | Skip setup wizard (`1` to skip) | `0` |

Example:
```bash
GMV_VERSION=0.2.0 GMV_NO_INIT=1 curl -fsSL .../install.sh | bash
```

---

## Manual Installation

### Download Binary

Download the pre-built binary for your platform from the
[Releases page](https://github.com/GMV-AI/gmv-agent/releases):

| Platform | Binary |
|----------|--------|
| macOS (Apple Silicon) | `gmv-agent-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `gmv-agent-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `gmv-agent-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (ARM64) | `gmv-agent-aarch64-unknown-linux-gnu.tar.gz` |

```bash
# Extract
tar -xzf gmv-agent-*.tar.gz

# Move to a directory in your PATH
sudo mv gmv-agent /usr/local/bin/
# or
mkdir -p ~/.gmv-agent/bin && mv gmv-agent ~/.gmv-agent/bin/

# Verify
gmv-agent --version
```

---

## Homebrew

```bash
brew tap GMV-AI/tap
brew install gmv-agent
```

To start as a background service:

```bash
brew services start gmv-agent
```

To upgrade:

```bash
brew upgrade gmv-agent
```

---

## Docker

### Using Docker Compose (recommended)

```bash
git clone https://github.com/GMV-AI/gmv-agent.git
cd gmv-agent
docker compose up -d
```

The `docker-compose.yml` mounts `~/.gmv-agent` for persistent configuration and data.

### Using Docker directly

```bash
# Build
docker build -t gmv-agent .

# Run interactive mode
docker run --rm -it \
  -v ~/.gmv-agent:/home/agent/.gmv-agent \
  -e OPENAI_API_KEY=sk-... \
  gmv-agent

# Run one-shot chat
docker run --rm \
  -v ~/.gmv-agent:/home/agent/.gmv-agent \
  -e OPENAI_API_KEY=sk-... \
  gmv-agent chat "What's on my calendar today?"
```

### Environment Variables in Docker

Pass API keys via environment variables or mount a secrets vault:

```bash
docker run --rm -it \
  -v ~/.gmv-agent:/home/agent/.gmv-agent \
  -e OPENAI_API_KEY=sk-... \
  -e TELEGRAM_BOT_TOKEN=... \
  -p 3001:3001 \
  -p 8443:8443 \
  gmv-agent serve --foreground
```

---

## Python Package

### Install from PyPI

```bash
pip install gmv-agent
```

> **Note:** The Rust binary must also be installed and on your `PATH`.
> The Python package wraps the native binary via PyO3 and also provides a CLI shim.

### Verify

```bash
python -c "from gmv_agent import GMVAgent; print('OK')"
gmv-agent --version
```

### Development Install

```bash
git clone https://github.com/GMV-AI/gmv-agent.git
cd gmv-agent/python
pip install maturin
maturin develop
pip install -e ".[dev]"
pytest
```

See [python/README.md](../python/README.md) for full Python usage documentation.

---

## Node.js Package

### Install from npm

```bash
# Global install (provides gmv-agent CLI)
npm install -g gmv-agent

# Or as a project dependency
npm install gmv-agent
```

> **Note:** The Rust binary must also be installed and on your `PATH`.
> The Node.js package wraps the native binary via NAPI-RS.

### Verify

```bash
node -e "const { GMVAgent } = require('gmv-agent'); console.log('OK')"
npx gmv-agent --version
```

### Development Install

```bash
git clone https://github.com/GMV-AI/gmv-agent.git
cd gmv-agent/node
npm install
npm run build
npm test
```

---

## Build from Source

### Prerequisites

- **Rust 1.75+**: Install via [rustup](https://rustup.rs/)
- **Git**: For cloning the repository

### Steps

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone the repository
git clone https://github.com/GMV-AI/gmv-agent.git
cd gmv-agent

# Build in release mode
cargo build --release

# The binary is at target/release/gmv-agent
./target/release/gmv-agent --version

# Optionally install system-wide
sudo cp target/release/gmv-agent /usr/local/bin/
```

### Build All Bindings

```bash
# Python bindings
cd python
pip install maturin
maturin develop
cd ..

# Node.js bindings
cd node
npm install
npm run build
cd ..
```

---

## Configuration

GMV Agent uses a layered configuration system:

1. **Environment variables** (highest priority)
2. **Encrypted secrets vault** (`~/.gmv-agent/secrets.enc`)
3. **TOML config files** (`config/default.toml` or `~/.gmv-agent/config.toml`)
4. **Built-in defaults** (lowest priority)

### Interactive Setup (Recommended)

```bash
gmv-agent init
```

This wizard guides you through:
1. **LLM Provider** — Choose OpenAI or Anthropic, enter API key
2. **Agent Identity** — Name and persona
3. **Integrations** — Google Calendar & Gmail, Telegram, WhatsApp
4. **Notifications** — Preferred notification channel
5. **Background Services** — Scheduler configuration

### Manual Configuration

#### Option A: Environment Variables (.env)

```bash
cp .env.example .env
# Edit .env with your API keys
```

Key variables:

| Variable | Description | Required |
|----------|-------------|----------|
| `OPENAI_API_KEY` | OpenAI API key | Yes (if using OpenAI) |
| `ANTHROPIC_API_KEY` | Anthropic API key | Yes (if using Anthropic) |
| `LLM_PROVIDER` | `openai` or `anthropic` | No (default: `openai`) |
| `LLM_MODEL` | Model name | No (auto-detected) |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID | For Calendar & Gmail |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret | For Calendar & Gmail |
| `TELEGRAM_BOT_TOKEN` | Telegram bot token | For Telegram |
| `TELEGRAM_DEFAULT_CHAT_ID` | Default Telegram chat ID | For Telegram |
| `TWILIO_ACCOUNT_SID` | Twilio account SID | For WhatsApp |
| `TWILIO_AUTH_TOKEN` | Twilio auth token | For WhatsApp |
| `TWILIO_WHATSAPP_FROM` | Twilio WhatsApp sender number | For WhatsApp |
| `AGENT_NAME` | Agent display name | No (default: GMV Agent) |
| `AGENT_PERSONA` | Agent personality description | No |

#### Option B: Encrypted Secrets Vault (Recommended)

After running `gmv-agent init`, secrets are stored encrypted at `~/.gmv-agent/secrets.enc`.

- **AES-256-GCM** encryption
- **Machine-bound** — encrypted with your machine's unique ID
- **No plaintext API keys** on disk

#### Option C: TOML Config File

Edit `config/default.toml` or create `~/.gmv-agent/config.toml`:

```toml
[agent]
name = "My Assistant"
persona = "You are a helpful personal assistant."
max_context_messages = 50
max_tool_iterations = 15

[llm]
provider = "openai"
model = "gpt-4o"
max_tokens = 4096
temperature = 0.7

[google_calendar]
enabled = true
redirect_port = 8085

[telegram]
enabled = true

[scheduler]
enabled = true
```

---

## First Run

### Interactive Mode (REPL)

```bash
gmv-agent
```

### One-Shot Query

```bash
gmv-agent chat "What's on my calendar today?"
```

### Check Configuration

```bash
# Diagnostic check
gmv-agent doctor

# Show status
gmv-agent status
```

### Available Commands

| Command | Description |
|---------|-------------|
| `gmv-agent` | Interactive mode (REPL) |
| `gmv-agent chat "..."` | One-shot query |
| `gmv-agent init` | Run setup wizard (`--reset` to start fresh, `--only <service>`) |
| `gmv-agent add <service>` | Add a service (telegram, google-calendar, whatsapp, github, slack) |
| `gmv-agent remove <service>` | Remove a service |
| `gmv-agent doctor` | Diagnostic check |
| `gmv-agent status` | Show agent status |
| `gmv-agent config list` | List current configuration |
| `gmv-agent config set <key> <value>` | Set a config value |
| `gmv-agent serve` | Start background daemon |
| `gmv-agent serve install` | Install as system service |
| `gmv-agent serve uninstall` | Remove system service |
| `gmv-agent jobs list` | List scheduled jobs |
| `gmv-agent jobs run <name>` | Run a job immediately |
| `gmv-agent jobs enable <name>` | Enable a job |
| `gmv-agent jobs disable <name>` | Disable a job |
| `gmv-agent tools` | List available tools |
| `gmv-agent telegram-bot` | Start Telegram bot mode |
| `gmv-agent logs` | Tail agent logs (`--scheduler` for scheduler logs) |

---

## Background Service

GMV Agent can run as a background daemon with scheduled jobs (RSVP monitoring, meeting reminders, daily briefing, calendar sync, token refresh, email digest).

### Scheduled Jobs

| Job | Default Schedule | Description |
|-----|-----------------|-------------|
| `reminder_check` | Every 1 min | Check and fire due reminders |
| `rsvp_monitor` | Every 15 min | Detect RSVP changes on calendar events |
| `meeting_reminder` | Every 5 min | Send upcoming meeting notifications |
| `calendar_sync` | Every 30 min | Sync calendar events |
| `token_refresh` | Every 45 min | Refresh OAuth tokens |
| `daily_briefing` | 8:00 AM | Morning summary of the day |
| `email_digest` | 6:00 PM | Evening email digest |

### Install as System Service

```bash
# macOS (launchd)
gmv-agent serve install

# This creates ~/Library/LaunchAgents/com.gmv.agent.plist
# and starts the service automatically
```

```bash
# Linux (systemd)
gmv-agent serve install

# This creates ~/.config/systemd/user/gmv-agent.service
# and enables + starts it
```

### Manage Service

```bash
# Check status
gmv-agent status

# View logs
gmv-agent logs

# List scheduled jobs
gmv-agent jobs list

# Run a job manually
gmv-agent jobs run <job-name>

# Uninstall
gmv-agent serve uninstall
```

---

## Upgrading

### Via Installer

```bash
curl -fsSL https://raw.githubusercontent.com/GMV-AI/gmv-agent/main/install.sh | bash
```

### Via Homebrew

```bash
brew upgrade gmv-agent
```

### Via pip / npm

```bash
pip install --upgrade gmv-agent
# or
npm update -g gmv-agent
```

### From Source

```bash
cd gmv-agent
git pull
cargo build --release
sudo cp target/release/gmv-agent /usr/local/bin/
```

Your configuration and secrets vault are preserved across upgrades.

---

## Uninstalling

```bash
# Remove system service (if installed)
gmv-agent serve uninstall

# Remove binary
rm ~/.gmv-agent/bin/gmv-agent
# or: sudo rm /usr/local/bin/gmv-agent
# or: brew uninstall gmv-agent
# or: pip uninstall gmv-agent
# or: npm uninstall -g gmv-agent

# Remove all data and configuration (optional)
rm -rf ~/.gmv-agent

# Remove PATH entry from shell rc file
# Edit ~/.zshrc or ~/.bashrc and remove the gmv-agent PATH line
```

---

## Troubleshooting

### Common Issues

**"No LLM API key configured"**
```bash
gmv-agent init   # Re-run setup wizard
# or set manually:
export OPENAI_API_KEY=sk-your-key
```

**"Failed to create data directory"**
```bash
mkdir -p ~/.gmv-agent/data
chmod 755 ~/.gmv-agent/data
```

**"Secrets file is corrupted"**
```bash
# Back up and recreate
mv ~/.gmv-agent/secrets.enc ~/.gmv-agent/secrets.enc.bak
gmv-agent init
```

**Google Calendar OAuth fails**
```bash
# Ensure redirect port is available
lsof -i :8085
# Try a different port
export GOOGLE_REDIRECT_PORT=9090
```

**Python/Node.js: "gmv-agent binary not found"**
```bash
# The Rust binary must be on your PATH
which gmv-agent
# If not found, install it:
curl -fsSL https://raw.githubusercontent.com/GMV-AI/gmv-agent/main/install.sh | bash
# Then restart your shell or source your profile:
source ~/.zshrc
```

### Diagnostic Check

```bash
gmv-agent doctor
```

This checks:
- LLM API key configuration
- Google Calendar & Gmail credentials
- Telegram bot token
- Data directory permissions
- Secrets vault integrity

### Logging

```bash
# Enable debug logging
RUST_LOG=debug gmv-agent

# View service logs
gmv-agent logs
```

### Getting Help

- Run `gmv-agent --help` for command reference
- Check [GitHub Issues](https://github.com/GMV-AI/gmv-agent/issues)
- See [CONTRIBUTING.md](../CONTRIBUTING.md) for development setup
