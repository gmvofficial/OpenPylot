# OpenPylot — Installation Guide

Complete reference for installing OpenPylot. For a 5-minute walk-through start with [GETTING-STARTED.md](./GETTING-STARTED.md); for running the server in production see [DEPLOYMENT.md](./DEPLOYMENT.md).

## Table of Contents

- [Quick Install](#quick-install)
- [Manual Installation](#manual-installation)
- [Homebrew](#homebrew)
- [Python Package](#python-package)
- [Node.js Package](#nodejs-package)
- [Docker](#docker)
- [Build from Source](#build-from-source)
- [First Run](#first-run)
- [Upgrading](#upgrading)
- [Uninstalling](#uninstalling)
- [Troubleshooting](#troubleshooting)

> Configuration (TOML, env vars, secrets vault) is documented separately in [CONFIGURATION.md](./CONFIGURATION.md).
> Running as a system service / scheduled jobs is documented in [DEPLOYMENT.md](./DEPLOYMENT.md).

---

## Quick Install

One-line installer (macOS / Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash
```

This will:

1. Detect your platform (macOS/Linux, x86_64/aarch64)
2. Download the latest release binary
3. Install to `~/.pylot/bin/`
4. Add to your PATH
5. Launch the interactive setup wizard

### Installer Options

| Variable        | Description                     | Default    |
| --------------- | ------------------------------- | ---------- |
| `PYLOT_VERSION` | Specific version to install     | `latest`   |
| `PYLOT_PREFIX`  | Installation directory          | `~/.pylot` |
| `PYLOT_NO_INIT` | Skip setup wizard (`1` to skip) | `0`        |

Example:

```bash
PYLOT_VERSION=0.1.0 PYLOT_NO_INIT=1 curl -fsSL .../install.sh | bash
```

---

## Manual Installation

### Download Binary

Download the pre-built binary for your platform from the
[Releases page](https://github.com/gmvofficial/OpenPylot/releases):

| Platform              | Binary                                   |
| --------------------- | ---------------------------------------- |
| macOS (Apple Silicon) | `pylot-aarch64-apple-darwin.tar.gz`      |
| macOS (Intel)         | `pylot-x86_64-apple-darwin.tar.gz`       |
| Linux (x86_64)        | `pylot-x86_64-unknown-linux-gnu.tar.gz`  |
| Linux (ARM64)         | `pylot-aarch64-unknown-linux-gnu.tar.gz` |

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
brew tap gmvofficial/tap
brew install openpylot
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

Quick try-out:

```bash
git clone https://github.com/gmvofficial/OpenPylot.git
cd pylot
docker compose up -d
```

The `docker-compose.yml` mounts `~/.pylot` for persistent configuration and data and publishes the API on port `3001`.

For production-grade Docker setup (env files, healthchecks, reverse proxy, persistent volumes, hardening checklist) see **[DEPLOYMENT.md](./DEPLOYMENT.md)**.

---

## Python Package

### Install from PyPI

```bash
pip install openpylot
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
git clone https://github.com/gmvofficial/OpenPylot.git
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
npm install -g openpylot

# Or as a project dependency
npm install openpylot
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
git clone https://github.com/gmvofficial/OpenPylot.git
cd pylot/node
npm install
npm run build
npm test
```

---

## Build from Source

### Prerequisites

- **Rust 1.75+** — install via [rustup](https://rustup.rs/)
- **Git**

### Steps

```bash
git clone https://github.com/gmvofficial/OpenPylot.git
cd pylot
cargo build --release

# Binary is at target/release/pylot
./target/release/pylot --version

# Optionally install system-wide
sudo cp target/release/pylot /usr/local/bin/
```

For building the Python wheel, the Node native module, and the frontend — plus the full contributor workflow — see **[DEVELOPMENT.md](./DEVELOPMENT.md)**.

---

## Configuration

OpenPylot resolves configuration in this order (highest priority first):

1. **Environment variables**
2. **Encrypted secrets vault** — `~/.pylot/secrets.enc`
3. **TOML config files** — `config/default.toml` or `~/.pylot/config.toml`
4. **Built-in defaults**

### Interactive setup (recommended)

```bash
pylot init
```

The wizard configures:

1. LLM provider + API key
2. Agent name & persona
3. Optional integrations (Google Calendar, Gmail, Telegram, WhatsApp)
4. Notification preferences
5. Background scheduler

All secrets are written encrypted to `~/.pylot/secrets.enc` (AES-256-GCM, Argon2id KDF, machine-bound).

For the full list of TOML keys, environment variables, social-platform credentials, and vault management commands see **[CONFIGURATION.md](./CONFIGURATION.md)** and **[SECURITY.md](./SECURITY.md)**.

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

| Command                          | Description                                                        |
| -------------------------------- | ------------------------------------------------------------------ |
| `pylot`                          | Interactive mode (REPL)                                            |
| `pylot chat "..."`               | One-shot query                                                     |
| `pylot init`                     | Run setup wizard (`--reset` to start fresh, `--only <service>`)    |
| `pylot add <service>`            | Add a service (telegram, google-calendar, whatsapp, github, slack) |
| `pylot remove <service>`         | Remove a service                                                   |
| `pylot doctor`                   | Diagnostic check                                                   |
| `pylot status`                   | Show agent status                                                  |
| `pylot config list`              | List current configuration                                         |
| `pylot config set <key> <value>` | Set a config value                                                 |
| `pylot serve`                    | Start background daemon                                            |
| `pylot serve install`            | Install as system service                                          |
| `pylot serve uninstall`          | Remove system service                                              |
| `pylot jobs list`                | List scheduled jobs                                                |
| `pylot jobs run <name>`          | Run a job immediately                                              |
| `pylot jobs enable <name>`       | Enable a job                                                       |
| `pylot jobs disable <name>`      | Disable a job                                                      |
| `pylot tools`                    | List available tools                                               |
| `pylot telegram-bot`             | Start Telegram bot mode                                            |
| `pylot logs`                     | Tail agent logs (`--scheduler` for scheduler logs)                 |

---

## Background Service

OpenPylot can run as a long-lived daemon that hosts the API + WebSocket server and runs cron-style background jobs (RSVP monitor, meeting reminders, daily briefing, calendar sync, token refresh, email digest).

Install as a system service:

```bash
# Foreground (Ctrl-C to stop)
pylot serve

# Install as launchd (macOS) / systemd (Linux)
pylot serve install
pylot serve uninstall
```

Manage:

```bash
pylot status            # connected services & uptime
pylot logs              # tail logs
pylot jobs list         # scheduled jobs
pylot jobs run <name>   # run a job immediately
```

Full production guide (Docker, systemd unit, reverse proxy, TLS, hardening) is in **[DEPLOYMENT.md](./DEPLOYMENT.md)**.

---

## Upgrading

### Via Installer

```bash
curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash
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
curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash
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
- Check [GitHub Issues](https://github.com/gmvofficial/OpenPylot/issues)
- See [CONTRIBUTING.md](../CONTRIBUTING.md) for development setup
