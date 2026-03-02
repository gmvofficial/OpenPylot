#!/usr/bin/env bash
# GMV Agent Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/user/gmv-agent/main/install.sh | bash
#
# Environment variables:
#   GMV_VERSION   - Specific version to install (default: latest)
#   GMV_PREFIX    - Installation prefix (default: ~/.gmv-agent)
#   GMV_NO_INIT   - Skip interactive setup (set to 1)
set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────────────────
REPO="user/gmv-agent"  # TODO: Update with actual GitHub repo
VERSION="${GMV_VERSION:-latest}"
PREFIX="${GMV_PREFIX:-$HOME/.gmv-agent}"
BIN_DIR="$PREFIX/bin"
DATA_DIR="$PREFIX/data"
LOGS_DIR="$PREFIX/logs"

# ─── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info()    { echo -e "${BLUE}ℹ${NC}  $*"; }
success() { echo -e "${GREEN}✓${NC}  $*"; }
warn()    { echo -e "${YELLOW}⚠${NC}  $*"; }
error()   { echo -e "${RED}✗${NC}  $*" >&2; }
fatal()   { error "$@"; exit 1; }

# ─── Platform Detection ──────────────────────────────────────────────────────
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="darwin" ;;
        *)       fatal "Unsupported OS: $(uname -s). Only Linux and macOS are supported." ;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)  arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)             fatal "Unsupported architecture: $(uname -m). Only x86_64 and aarch64 are supported." ;;
    esac

    PLATFORM="${os}"
    ARCH="${arch}"
    TARGET="${arch}-${os}"

    if [[ "$os" == "linux" ]]; then
        TARGET="${arch}-unknown-linux-gnu"
    elif [[ "$os" == "darwin" ]]; then
        TARGET="${arch}-apple-darwin"
    fi
}

# ─── Dependency Check ─────────────────────────────────────────────────────────
check_dependencies() {
    local missing=()
    for cmd in curl tar; do
        if ! command -v "$cmd" &>/dev/null; then
            missing+=("$cmd")
        fi
    done
    if [[ ${#missing[@]} -gt 0 ]]; then
        fatal "Missing required commands: ${missing[*]}. Please install them and retry."
    fi
}

# ─── Resolve Version ─────────────────────────────────────────────────────────
resolve_version() {
    if [[ "$VERSION" == "latest" ]]; then
        info "Fetching latest release..."
        VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
            | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/')

        if [[ -z "$VERSION" ]]; then
            fatal "Could not determine latest version. Set GMV_VERSION manually."
        fi
    fi
    info "Installing gmv-agent v${VERSION} for ${TARGET}"
}

# ─── Download & Install Binary ───────────────────────────────────────────────
install_binary() {
    local url="https://github.com/${REPO}/releases/download/v${VERSION}/gmv-agent-${TARGET}.tar.gz"
    local tmp_dir

    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    info "Downloading from ${url}..."
    if ! curl -fsSL "$url" -o "$tmp_dir/gmv-agent.tar.gz" 2>/dev/null; then
        warn "Pre-built binary not available for ${TARGET}."
        info "Attempting to build from source..."
        build_from_source
        return
    fi

    info "Extracting..."
    tar -xzf "$tmp_dir/gmv-agent.tar.gz" -C "$tmp_dir"

    # Find the binary
    local binary
    binary=$(find "$tmp_dir" -name "gmv-agent" -type f | head -1)
    if [[ -z "$binary" ]]; then
        fatal "Binary not found in archive."
    fi

    mkdir -p "$BIN_DIR"
    mv "$binary" "$BIN_DIR/gmv-agent"
    chmod +x "$BIN_DIR/gmv-agent"
    success "Binary installed to ${BIN_DIR}/gmv-agent"
}

# ─── Build from Source (fallback) ────────────────────────────────────────────
build_from_source() {
    if ! command -v cargo &>/dev/null; then
        info "Rust not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source "$HOME/.cargo/env"
    fi

    info "Building gmv-agent from source (this may take a few minutes)..."
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    git clone --depth 1 --branch "v${VERSION}" "https://github.com/${REPO}.git" "$tmp_dir/gmv-agent" 2>/dev/null \
        || git clone --depth 1 "https://github.com/${REPO}.git" "$tmp_dir/gmv-agent"

    cd "$tmp_dir/gmv-agent"

    # Build frontend if Node.js is available
    build_frontend "$tmp_dir/gmv-agent/frontend" 2>/dev/null || true

    cargo build --release

    mkdir -p "$BIN_DIR"
    cp target/release/gmv-agent "$BIN_DIR/gmv-agent"
    chmod +x "$BIN_DIR/gmv-agent"
    success "Built and installed to ${BIN_DIR}/gmv-agent"
}

# ─── Build Frontend ──────────────────────────────────────────────────────────
build_frontend() {
    local frontend_dir="$1"

    # Check for Node.js / npm
    if ! command -v node &>/dev/null || ! command -v npm &>/dev/null; then
        warn "Node.js not found. Skipping frontend build."
        info "Install Node.js 18+ and run: cd frontend && npm ci && npm run build"
        return 0
    fi

    local node_version
    node_version=$(node --version | sed 's/v//' | cut -d. -f1)
    if [[ "$node_version" -lt 18 ]]; then
        warn "Node.js 18+ required (found v${node_version}). Skipping frontend build."
        return 0
    fi

    info "Building frontend..."
    if [[ -d "$frontend_dir" && -f "$frontend_dir/package.json" ]]; then
        cd "$frontend_dir"
        npm ci --no-audit --no-fund 2>/dev/null || npm install --no-audit --no-fund
        npm run build
        cd - >/dev/null

        # Copy built frontend to installation directory
        if [[ -d "$frontend_dir/out" ]]; then
            mkdir -p "$PREFIX/frontend/out"
            cp -r "$frontend_dir/out/"* "$PREFIX/frontend/out/"
            success "Frontend built and installed to ${PREFIX}/frontend/out"
        else
            warn "Frontend build did not produce output directory."
        fi
    else
        warn "Frontend directory not found at $frontend_dir"
    fi
}

# ─── Setup Directories ───────────────────────────────────────────────────────
setup_directories() {
    mkdir -p "$PREFIX" "$DATA_DIR" "$LOGS_DIR"
    success "Created directories: ${PREFIX}/{data,logs}"
}

# ─── Configure PATH ──────────────────────────────────────────────────────────
configure_path() {
    local shell_rc=""
    local path_entry="export PATH=\"${BIN_DIR}:\$PATH\""

    # Detect shell
    case "${SHELL:-/bin/bash}" in
        */zsh)  shell_rc="$HOME/.zshrc" ;;
        */bash)
            if [[ -f "$HOME/.bash_profile" ]]; then
                shell_rc="$HOME/.bash_profile"
            else
                shell_rc="$HOME/.bashrc"
            fi
            ;;
        */fish)
            shell_rc="$HOME/.config/fish/config.fish"
            path_entry="set -gx PATH ${BIN_DIR} \$PATH"
            ;;
        *)
            warn "Unknown shell. Add ${BIN_DIR} to your PATH manually."
            return
            ;;
    esac

    # Check if already in PATH
    if echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_DIR"; then
        return
    fi

    # Check if already in rc file
    if [[ -f "$shell_rc" ]] && grep -qF "$BIN_DIR" "$shell_rc"; then
        return
    fi

    echo "" >> "$shell_rc"
    echo "# GMV Agent" >> "$shell_rc"
    echo "$path_entry" >> "$shell_rc"
    success "Added ${BIN_DIR} to PATH in ${shell_rc}"
    warn "Run 'source ${shell_rc}' or open a new terminal to use gmv-agent."
}

# ─── Run Interactive Setup ────────────────────────────────────────────────────
run_init() {
    if [[ "${GMV_NO_INIT:-0}" == "1" ]]; then
        info "Skipping interactive setup (GMV_NO_INIT=1)."
        info "Run 'gmv-agent init' later to configure."
        return
    fi

    echo ""
    echo -e "${BOLD}─── Setup ───${NC}"
    echo ""

    # Use the binary we just installed
    export PATH="${BIN_DIR}:$PATH"
    if command -v gmv-agent &>/dev/null; then
        gmv-agent init
    else
        warn "Could not run 'gmv-agent init'. Run it manually after adding ${BIN_DIR} to PATH."
    fi
}

# ─── Print Summary ────────────────────────────────────────────────────────────
print_summary() {
    echo ""
    echo -e "${BOLD}${GREEN}━━━ GMV Agent Installed Successfully ━━━${NC}"
    echo ""
    echo -e "  Binary:     ${BIN_DIR}/gmv-agent"
    echo -e "  Data:       ${DATA_DIR}"
    echo -e "  Logs:       ${LOGS_DIR}"
    echo -e "  Version:    ${VERSION}"
    echo ""
    echo -e "  ${BOLD}Quick start:${NC}"
    echo -e "    gmv-agent chat \"Hello!\"        # One-shot query"
    echo -e "    gmv-agent                       # Interactive mode"
    echo -e "    gmv-agent serve --foreground    # Start web UI + scheduler"
    echo -e "    gmv-agent init                  # Re-run setup wizard"
    echo -e "    gmv-agent doctor                # Check configuration"
    echo ""
    echo -e "  ${BOLD}Web UI:${NC}"
    echo -e "    Run 'gmv-agent serve' and open http://localhost:3001"
    echo ""
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
    echo -e "${BOLD}${BLUE}"
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║       GMV Agent Installer         ║"
    echo "  ╚═══════════════════════════════════╝"
    echo -e "${NC}"

    check_dependencies
    detect_platform
    resolve_version
    setup_directories
    install_binary
    build_frontend "$PREFIX/src/frontend" 2>/dev/null || true
    configure_path
    run_init
    print_summary
}

main "$@"
