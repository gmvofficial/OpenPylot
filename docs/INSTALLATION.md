# OpenPylot — Installation Guide

Complete guide for installing, configuring, and running OpenPylot.

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
curl -fsSL https://raw.githubusercontent.com/openpylot/pylot/main/install.sh | bash
```

This will:
1. Detect your platform (macOS/Linux, x86_64/aarch64)
2. Download the latest release binary
3. Install to `~/.pylot/bin/`
4. Add to your PATH
5. Launch the interactive setup wizard

### Installer Options

| Variable | Description | Default |
|----------|-------------|---------|
| `PYLOT_VERSION` | Specific version to install | `latest` |
| `PYLOT_PREFIX` | Installation directory | `~/.pylot` |
| `PYLOT_NO_INIT` | Skip setup wizard (`1` to skip) | `0` |

Example:
```bash
PYLOT_VERSION=0.2.0 PYLOT_NO_INIT=1 curl -fsSL .../install.sh | bash
```

---

## Manual Installation

### Download Binary

Download the pre-built binary for your platform from the
[Releases page](https://github.com/openpylot/pylot/releases):

| Platform | Binary |
|----------|--------|
| macOS (Apple Silicon) | `pylot-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `pylot-x86_64-apple-darwin.tar.gz` |
| Linux (x86_64) | `pylot-x86_64-unknown-linux-gnu.tar.gz` |
| Linux (ARM64) | `pylot-aarch64-unknown-linux-gnu.tar.gz` |

```bash
# Extract
tar -xzf pylot-*.tar.gz

# Move to a directory in your PATH
sudo mv pylot /usr/local/bin/
# or
mkdir -p ~/.pylot/bin && mv pylot ~/.pylot/bin/

# Verify
pylot --version
```

---

## Homebrew

```bash
brew tap openpylot/tap
brew install pylot
```

To start as a background service:

```bash
brew services start pylot
```

To upgrade:

```bash
brew upgrade pylot
```

---

## Docker

### Using Docker Compose (recommended)

```bash
git clone https://github.com/openpylot/pylot.git
cd pylot
docker compose up -d
```

The `docker-compose.yml` mounts `~/.pylot` for persistent configuration and data.

### Using Docker directly

```bash
# Build
docker build -t pylot .

# Run interactive mode
docker run --rm -it \
  -v ~/.pylot:/home/pylot/.pylot \
  -e OPENAI_API_KEY=sk-... \
  pylot

# Run one-shot chat
docker run --rm \
  -v ~/.pylot:/home/pylot/.pylot \
  -e OPENAI_API_KEY=sk-... \
  pylot chat "What's on my calendar today?"
```

### Environment Variables in Docker

Pass API keys via environment variables or mount a secrets vault:

```bash
docker run --rm -it \
  -v ~/.pylot:/home/pylot/.pylot \
  -e OPENAI_API_KEY=sk-... \
  -e TELEGRAM_BOT_TOKEN=... \
  -p 3001:3001 \
  -p 8443:8443 \
  pylot serve --foreground
```

---

## Python Package

### Install from PyPI

```bash
pip install pylot
```

> **Note:** The Rust binary must also be installed and on your `PATH`.
> The Python package wraps the native binary via PyO3 and also provides a CLI shim.

### Verify

```bash
python -c "from pylot import PylotAgent; print('OK')"
pylot --version
```

### Development Install

```bash
git clone https://github.com/openpylot/pylot.git
cd pylot/python
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
# Global install (provides pylot CLI)
npm install -g pylot

# Or as a project dependency
npm install pylot
```

> **Note:** The Rust binary must also be installed and on your `PATH`.
> The Node.js package wraps the native binary via NAPI-RS.

### Verify

```bash
node -e "const { PylotAgent } = require('pylot'); console.log('OK')"
npx pylot --version
```

### Development Install

```bash
git clone https://github.com/openpylot/pylot.git
cd pylot/node
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
git clone https://github.com/openpylot/pylot.git
cd pylot

# Build in release mode
cargo build --release

# The binary is at target/release/pylot
./target/release/pylot --version

# Optionally install system-wide
sudo cp target/release/pylot /usr/local/bin/
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

OpenPylot uses a layered configuration system:

1. **Environment variables** (highest priority)
2. **Encrypted secrets vault** (`~/.pylot/secrets.enc`)
3. **TOML config files** (`config/default.toml` or `~/.pylot/config.toml`)
4. **Built-in defaults** (lowest priority)

### Interactive Setup (Recommended)

```bash
pylot init
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
| `AGENT_NAME` | Agent display name | No (default: Pylot) |
| `AGENT_PERSONA` | Agent personality description | No |

#### Option B: Encrypted Secrets Vault (Recommended)

After running `pylot init`, secrets are stored encrypted at `~/.pylot/secrets.enc`.

- **AES-256-GCM** encryption
- **Machine-bound** — encrypted with your machine's unique ID
- **No plaintext API keys** on disk

#### Option C: TOML Config File

Edit `config/default.toml` or create `~/.pylot/config.toml`:

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
pylot
```

### One-Shot Query

```bash
pylot chat "What's on my calendar today?"
```

### Check Configuration

```bash
# Diagnostic check
pylot doctor

# Show status
pylot status
```

### Available Commands

| Command | Description |
|---------|-------------|
| `pylot` | Interactive mode (REPL) |
| `pylot chat "..."` | One-shot query |
| `pylot init` | Run setup wizard (`--reset` to start fresh, `--only <service>`) |
| `pylot add <service>` | Add a service (telegram, google-calendar, whatsapp, github, slack) |
| `pylot remove <service>` | Remove a service |
| `pylot doctor` | Diagnostic check |
| `pylot status` | Show agent status |
| `pylot config list` | List current configuration |
| `pylot config set <key> <value>` | Set a config value |
| `pylot serve` | Start background daemon |
| `pylot serve install` | Install as system service |
| `pylot serve uninstall` | Remove system service |
| `pylot jobs list` | List scheduled jobs |
| `pylot jobs run <name>` | Run a job immediately |
| `pylot jobs enable <name>` | Enable a job |
| `pylot jobs disable <name>` | Disable a job |
| `pylot tools` | List available tools |
| `pylot telegram-bot` | Start Telegram bot mode |
| `pylot logs` | Tail agent logs (`--scheduler` for scheduler logs) |

---

## Background Service

OpenPylot can run as a background daemon with scheduled jobs (RSVP monitoring, meeting reminders, daily briefing, calendar sync, token refresh, email digest).

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
pylot serve install

# This creates ~/Library/LaunchAgents/com.openpylot.agent.plist
# and starts the service automatically
```

```bash
# Linux (systemd)
pylot serve install

# This creates ~/.config/systemd/user/pylot.service
# and enables + starts it
```

### Manage Service

```bash
# Check status
pylot status

# View logs
pylot logs

# List scheduled jobs
pylot jobs list

# Run a job manually
pylot jobs run <job-name>

# Uninstall
pylot serve uninstall
```

---

## Upgrading

### Via Installer

```bash
curl -fsSL https://raw.githubusercontent.com/openpylot/pylot/main/install.sh | bash
```

### Via Homebrew

```bash
brew upgrade pylot
```

### Via pip / npm

```bash
pip install --upgrade pylot
# or
npm update -g pylot
```

### From Source

```bash
cd pylot
git pull
cargo build --release
sudo cp target/release/pylot /usr/local/bin/
```

Your configuration and secrets vault are preserved across upgrades.

---

## Uninstalling

```bash
# Remove system service (if installed)
pylot serve uninstall

# Remove binary
rm ~/.pylot/bin/pylot
# or: sudo rm /usr/local/bin/pylot
# or: brew uninstall pylot
# or: pip uninstall pylot
# or: npm uninstall -g pylot

# Remove all data and configuration (optional)
rm -rf ~/.pylot

# Remove PATH entry from shell rc file
# Edit ~/.zshrc or ~/.bashrc and remove the pylot PATH line
```

---

## Troubleshooting

### Common Issues

**"No LLM API key configured"**
```bash
pylot init   # Re-run setup wizard
# or set manually:
export OPENAI_API_KEY=sk-your-key
```

**"Failed to create data directory"**
```bash
mkdir -p ~/.pylot/data
chmod 755 ~/.pylot/data
```

**"Secrets file is corrupted"**
```bash
# Back up and recreate
mv ~/.pylot/secrets.enc ~/.pylot/secrets.enc.bak
pylot init
```

**Google Calendar OAuth fails**
```bash
# Ensure redirect port is available
lsof -i :8085
# Try a different port
export GOOGLE_REDIRECT_PORT=9090
```

**Python/Node.js: "pylot binary not found"**
```bash
# The Rust binary must be on your PATH
which pylot
# If not found, install it:
curl -fsSL https://raw.githubusercontent.com/openpylot/pylot/main/install.sh | bash
# Then restart your shell or source your profile:
source ~/.zshrc
```

### Diagnostic Check

```bash
pylot doctor
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
RUST_LOG=debug pylot

# View service logs
pylot logs
```

### Getting Help

- Run `pylot --help` for command reference
- Check [GitHub Issues](https://github.com/openpylot/pylot/issues)
- See [CONTRIBUTING.md](../CONTRIBUTING.md) for development setup
