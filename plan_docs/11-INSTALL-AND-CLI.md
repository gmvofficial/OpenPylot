# 11 — One-Liner Install & CLI Polish

## Objective

Make OpenPylot installable with a single command on macOS/Linux/Windows. Add shell completions, a `doctor` diagnostic command, improved CLI UX, and `/slash` commands in the REPL.

---

## Current State

- **Install methods**: Homebrew (Formula/pylot.rb), Docker, `install.sh`, pip, npm, from source
- **CLI**: Basic REPL in `src/terminal.rs`, `pylot init` wizard
- **Missing**: One-liner curl install, shell completions, doctor, slash commands

---

## Reference Implementations

### IronClaw
- **One-liner**: `curl ... | sh && ironclaw onboard` 
- **Shell completions**: bash/zsh/fish via `ironclaw completion`
- **Doctor**: 16 diagnostic checks (settings, LLM, DB, embeddings, etc.)
- **CLI commands**: 15+ subcommands (run, onboard, tool, config, memory, status, skills, etc.)

### OpenClaw
- **One-liner**: `npm install -g openclaw@latest && openclaw onboard --install-daemon`
- **Doctor**: `openclaw doctor`, `openclaw security audit`

---

## Implementation Steps

### Step 1: One-liner install script (Day 1)

**File**: Update `install.sh`

```bash
#!/usr/bin/env bash
set -euo pipefail

# OpenPylot Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/openpylot/openpylot/main/install.sh | bash

REPO="openpylot/openpylot"
INSTALL_DIR="${PYLOT_INSTALL_DIR:-$HOME/.pylot/bin}"
CONFIG_DIR="$HOME/.pylot"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}[info]${NC} $1"; }
success() { echo -e "${GREEN}[ok]${NC} $1"; }
error() { echo -e "${RED}[error]${NC} $1"; exit 1; }

# Detect OS and arch
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    linux) OS="linux" ;;
    darwin) OS="macos" ;;
    *) error "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    *) error "Unsupported architecture: $ARCH" ;;
esac

BINARY="pylot-${OS}-${ARCH}"

# Get latest release
info "Fetching latest release..."
LATEST=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST}/${BINARY}.tar.gz"

# Download and install
info "Downloading OpenPylot ${LATEST} for ${OS}/${ARCH}..."
mkdir -p "$INSTALL_DIR"
curl -fsSL "$DOWNLOAD_URL" | tar -xz -C "$INSTALL_DIR"
chmod +x "${INSTALL_DIR}/pylot"

# Add to PATH
SHELL_NAME="$(basename "$SHELL")"
PROFILE=""
case "$SHELL_NAME" in
    zsh) PROFILE="$HOME/.zshrc" ;;
    bash) PROFILE="$HOME/.bashrc" ;;
    fish) PROFILE="$HOME/.config/fish/config.fish" ;;
esac

if [ -n "$PROFILE" ]; then
    if ! grep -q "PYLOT_INSTALL_DIR\|\.pylot/bin" "$PROFILE" 2>/dev/null; then
        echo "" >> "$PROFILE"
        echo "# OpenPylot" >> "$PROFILE"
        echo "export PATH=\"${INSTALL_DIR}:\$PATH\"" >> "$PROFILE"
        info "Added ${INSTALL_DIR} to PATH in ${PROFILE}"
    fi
fi

# Create config dir
mkdir -p "$CONFIG_DIR"

success "OpenPylot ${LATEST} installed successfully!"
echo ""
echo "  Run: pylot init    # Setup wizard"
echo "  Run: pylot         # Start chatting"
echo ""
echo "  Restart your shell or run: export PATH=\"${INSTALL_DIR}:\$PATH\""
```

### Step 2: Windows PowerShell installer (Day 1)

**File**: Create `install.ps1`

```powershell
# OpenPylot Windows Installer
# Usage: irm https://raw.githubusercontent.com/openpylot/openpylot/main/install.ps1 | iex

$ErrorActionPreference = "Stop"
$Repo = "openpylot/openpylot"
$InstallDir = "$env:USERPROFILE\.pylot\bin"

Write-Host "[info] Fetching latest release..." -ForegroundColor Cyan
$Latest = (Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest").tag_name
$Arch = if ([System.Environment]::Is64BitOperatingSystem) { "x86_64" } else { "x86" }
$Url = "https://github.com/$Repo/releases/download/$Latest/pylot-windows-$Arch.zip"

Write-Host "[info] Downloading OpenPylot $Latest..." -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$TempZip = "$env:TEMP\pylot.zip"
Invoke-WebRequest -Uri $Url -OutFile $TempZip
Expand-Archive -Path $TempZip -DestinationPath $InstallDir -Force
Remove-Item $TempZip

# Add to PATH
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$InstallDir;$UserPath", "User")
    Write-Host "[info] Added to PATH" -ForegroundColor Cyan
}

Write-Host "[ok] OpenPylot $Latest installed!" -ForegroundColor Green
Write-Host ""
Write-Host "  Run: pylot init    # Setup wizard"
Write-Host "  Run: pylot         # Start chatting"
```

### Step 3: Shell completions (Day 1)

**File**: Modify `src/main.rs` — Add `pylot completion` subcommand

Using `clap_complete`:

```rust
// In Cargo.toml: clap = { version = "4", features = ["derive"] }
//                clap_complete = "4"

use clap::{Parser, Subcommand};
use clap_complete::{generate, shells};

#[derive(Subcommand)]
pub enum Commands {
    /// Start interactive REPL
    Chat,
    /// Setup wizard
    Init,
    /// Start API server
    Serve,
    /// Manage skills
    Skills {
        #[command(subcommand)]
        action: SkillsAction,
    },
    /// Manage sub-agents
    Agents {
        #[command(subcommand)]
        action: AgentsAction,
    },
    /// Manage MCP servers
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Memory operations
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Social media management
    Social {
        #[command(subcommand)]
        action: SocialAction,
    },
    /// Learning system management
    Learn {
        #[command(subcommand)]
        action: LearnAction,
    },
    /// System diagnostics
    Doctor,
    /// Show system status
    Status,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Generate shell completions
    Completion {
        #[arg(value_enum)]
        shell: shells::Shell,
    },
    /// Show version
    Version,
}

// Handler for completion:
fn handle_completion(shell: shells::Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "pylot", &mut std::io::stdout());
}
```

**Usage**:
```bash
# Zsh
pylot completion zsh > ~/.pylot/_pylot
echo 'fpath=(~/.pylot $fpath)' >> ~/.zshrc

# Bash
pylot completion bash > /etc/bash_completion.d/pylot

# Fish
pylot completion fish > ~/.config/fish/completions/pylot.fish
```

### Step 4: Doctor diagnostic command (Day 2)

**File**: Create `src/doctor.rs`

```rust
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

pub enum CheckStatus {
    Ok,
    Warning,
    Error,
}

pub async fn run_doctor(config: &AppConfig) -> Vec<DoctorCheck> {
    let mut checks = vec![];

    // 1. Config file
    checks.push(check_config_exists().await);

    // 2. Secrets vault
    checks.push(check_secrets_vault().await);

    // 3. LLM provider connectivity
    checks.push(check_llm_provider(config).await);

    // 4. Database (SQLite)
    checks.push(check_database(config).await);

    // 5. Embeddings
    checks.push(check_embeddings(config).await);

    // 6. Memory system
    checks.push(check_memory(config).await);

    // 7. Skills directory
    checks.push(check_skills_dir().await);

    // 8. OAuth tokens (Google, GitHub, Slack)
    checks.push(check_oauth_tokens(config).await);

    // 9. Scheduler
    checks.push(check_scheduler(config).await);

    // 10. MCP servers
    checks.push(check_mcp_servers(config).await);

    // 11. API server port availability
    checks.push(check_api_port(config).await);

    // 12. Python SDK
    checks.push(check_python_sdk().await);

    // 13. Node.js SDK
    checks.push(check_node_sdk().await);

    // 14. Social media connections
    checks.push(check_social_platforms(config).await);

    // 15. Disk space
    checks.push(check_disk_space().await);

    // 16. Version check
    checks.push(check_latest_version().await);

    checks
}

async fn check_llm_provider(config: &AppConfig) -> DoctorCheck {
    // Try a simple API call to the configured provider
    match crate::llm::test_connection(config).await {
        Ok(_) => DoctorCheck {
            name: "LLM Provider".to_string(),
            status: CheckStatus::Ok,
            message: format!("Connected to {} ({})", config.llm.provider, config.llm.model),
        },
        Err(e) => DoctorCheck {
            name: "LLM Provider".to_string(),
            status: CheckStatus::Error,
            message: format!("Cannot connect: {}", e),
        },
    }
}
// ... similar for each check
```

### Step 5: REPL slash commands (Day 2)

**File**: Modify `src/terminal.rs`

Add slash commands in the interactive REPL:

```rust
fn handle_slash_command(input: &str, agent: &Agent) -> Option<String> {
    let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
    let cmd = parts[0];
    let args = parts.get(1).unwrap_or(&"");

    match cmd {
        "/help" => Some(format!(
            "Available commands:\n\
             /help          — Show this help\n\
             /status        — System status\n\
             /model         — Show/change model\n\
             /memory        — Search memory\n\
             /skills        — List active skills\n\
             /agents        — List sub-agents\n\
             /social        — Social media status\n\
             /config        — Show configuration\n\
             /clear         — Clear conversation\n\
             /compact       — Compact context\n\
             /cost          — Show token usage\n\
             /export        — Export conversation\n\
             /quit          — Exit"
        )),
        "/status" => Some(get_status(agent)),
        "/model" => {
            if args.is_empty() {
                Some(format!("Current model: {}", agent.config.llm.model))
            } else {
                agent.set_model(args);
                Some(format!("Model changed to: {}", args))
            }
        },
        "/memory" => Some(search_memory(agent, args)),
        "/skills" => Some(list_skills(agent)),
        "/agents" => Some(list_agents(agent)),
        "/clear" => { agent.clear_context(); Some("Conversation cleared.".to_string()) },
        "/cost" => Some(format!("Tokens used: {} input, {} output", agent.usage.input, agent.usage.output)),
        "/quit" | "/exit" => std::process::exit(0),
        _ => None,  // Not a command, treat as regular message
    }
}
```

### Step 6: Update Homebrew formula (Day 2)

**File**: `Formula/pylot.rb`

Ensure the formula builds correctly and includes all new features:

```ruby
class Pylot < Formula
  desc "OpenPylot - AI Assistant with Smart Memory & Learning"
  homepage "https://github.com/openpylot/openpylot"
  url "https://github.com/openpylot/openpylot/archive/refs/tags/v1.0.0.tar.gz"
  sha256 "..."
  license "MIT"

  depends_on "rust" => :build
  depends_on "openssl"

  def install
    system "cargo", "build", "--release"
    bin.install "target/release/pylot"

    # Install shell completions
    generate_completions_from_executable(bin/"pylot", "completion")

    # Install bundled skills
    (share/"pylot/skills").install Dir["skills/*"]
  end

  test do
    assert_match "OpenPylot", shell_output("#{bin}/pylot version")
  end
end
```

---

## Config Additions

None — this doc enhances existing commands and install infrastructure.

---

## Testing

- `test_install_script` — Script exits cleanly on supported OS
- `test_shell_completions` — Valid completion output for bash/zsh/fish
- `test_doctor_checks` — All 16 checks run without panic
- `test_slash_commands` — Each command produces expected output
- `test_cli_subcommands` — All subcommands parse correctly

---

## Acceptance Criteria

- [ ] `curl -fsSL ... | bash` installs pylot on macOS/Linux
- [ ] PowerShell script installs on Windows
- [ ] `pylot completion zsh/bash/fish` generates valid completions
- [ ] `pylot doctor` runs 16 diagnostic checks
- [ ] REPL supports /help, /status, /model, /memory, /skills, /agents, /clear, /cost, /quit
- [ ] All new subcommands registered (skills, agents, mcp, social, learn, doctor, status, completion)
- [ ] Homebrew formula builds and installs correctly
- [ ] Binary added to PATH automatically
