#!/usr/bin/env bash
# OpenPylot Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash
#
# Environment variables:
#   PYLOT_VERSION   - Specific version to install (default: latest)
#   PYLOT_PREFIX    - Installation prefix (default: ~/.pylot)
#   PYLOT_NO_INIT   - Skip interactive setup (set to 1)
set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────────────────
REPO="gmvofficial/OpenPylot"       # GitHub repo (owner/name)
CRATE="openpylot"                  # crates.io package (builds the `pylot` binary)
VERSION="${PYLOT_VERSION:-latest}"
PREFIX="${PYLOT_PREFIX:-$HOME/.pylot}"
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

    if [[ "$os" == "linux" ]]; then
        TARGET="${arch}-unknown-linux-gnu"
    else
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
        VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed -E 's/.*"v?([^"]+)".*/\1/' || true)

        if [[ -z "$VERSION" ]]; then
            warn "Could not determine latest GitHub release; will install the latest published crate."
            VERSION="latest"
        fi
    fi
    if [[ "$VERSION" == "latest" ]]; then
        info "Installing pylot (latest) for ${TARGET}"
    else
        info "Installing pylot v${VERSION} for ${TARGET}"
    fi
}

# ─── Download & Install Binary ───────────────────────────────────────────────
install_binary() {
    # Prebuilt binaries are only available for tagged releases.
    if [[ "$VERSION" == "latest" ]]; then
        info "No pinned version — installing from source via cargo."
        install_from_cargo
        return
    fi

    local url="https://github.com/${REPO}/releases/download/v${VERSION}/pylot-${TARGET}.tar.gz"
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    info "Downloading prebuilt binary from ${url}..."
    if ! curl -fsSL "$url" -o "$tmp_dir/pylot.tar.gz" 2>/dev/null; then
        warn "Pre-built binary not available for ${TARGET}."
        install_from_cargo
        return
    fi

    info "Extracting..."
    tar -xzf "$tmp_dir/pylot.tar.gz" -C "$tmp_dir"

    local binary
    binary=$(find "$tmp_dir" -name "pylot" -type f | head -1)
    if [[ -z "$binary" ]]; then
        warn "Binary not found in archive; falling back to cargo."
        install_from_cargo
        return
    fi

    mkdir -p "$BIN_DIR"
    mv "$binary" "$BIN_DIR/pylot"
    chmod +x "$BIN_DIR/pylot"
    success "Binary installed to ${BIN_DIR}/pylot"
}

# ─── Install from crates.io (source build fallback) ──────────────────────────
install_from_cargo() {
    if ! command -v cargo &>/dev/null; then
        info "Rust toolchain not found. Installing via rustup..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    fi

    info "Building pylot from source with cargo (this may take a few minutes)..."
    mkdir -p "$PREFIX"

    local ver_args=()
    if [[ "$VERSION" != "latest" ]]; then
        ver_args=(--version "$VERSION")
    fi

    # Prefer the pinned lockfile for reproducibility; retry without it if the
    # published crate didn't ship a Cargo.lock.
    if ! cargo install "$CRATE" "${ver_args[@]}" --root "$PREFIX" --locked --force 2>/dev/null; then
        cargo install "$CRATE" "${ver_args[@]}" --root "$PREFIX" --force
    fi

    if [[ -x "$BIN_DIR/pylot" ]]; then
        success "Built and installed pylot to ${BIN_DIR}/pylot"
    else
        fatal "cargo install did not produce ${BIN_DIR}/pylot"
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
    echo "# OpenPylot" >> "$shell_rc"
    echo "$path_entry" >> "$shell_rc"
    success "Added ${BIN_DIR} to PATH in ${shell_rc}"
    warn "Run 'source ${shell_rc}' or open a new terminal to use pylot."
}

# ─── Run Interactive Setup ────────────────────────────────────────────────────
run_init() {
    if [[ "${PYLOT_NO_INIT:-0}" == "1" ]]; then
        info "Skipping interactive setup (PYLOT_NO_INIT=1)."
        info "Run 'pylot init' later to configure."
        return
    fi

    # Running via `curl | bash` gives us no TTY for interactive prompts.
    if [[ ! -t 0 ]]; then
        info "Non-interactive shell detected — skipping the setup wizard."
        info "Run 'pylot init' after installation to configure."
        return
    fi

    echo ""
    echo -e "${BOLD}─── Setup ───${NC}"
    echo ""

    # Use the binary we just installed
    export PATH="${BIN_DIR}:$PATH"
    if command -v pylot &>/dev/null; then
        pylot init
    else
        warn "Could not run 'pylot init'. Run it manually after adding ${BIN_DIR} to PATH."
    fi
}

# ─── Print Summary ────────────────────────────────────────────────────────────
print_summary() {
    echo ""
    echo -e "${BOLD}${GREEN}━━━ OpenPylot Installed Successfully ━━━${NC}"
    echo ""
    echo -e "  Binary:     ${BIN_DIR}/pylot"
    echo -e "  Data:       ${DATA_DIR}"
    echo -e "  Logs:       ${LOGS_DIR}"
    echo -e "  Version:    ${VERSION}"
    echo ""
    echo -e "  ${BOLD}Quick start:${NC}"
    echo -e "    pylot chat \"Hello!\"        # One-shot query"
    echo -e "    pylot                       # Interactive mode"
    echo -e "    pylot serve --foreground    # Start web UI + scheduler"
    echo -e "    pylot init                  # Re-run setup wizard"
    echo -e "    pylot doctor                # Check configuration"
    echo ""
    echo -e "  ${BOLD}Web UI:${NC}"
    echo -e "    Run 'pylot serve' and open http://localhost:3001"
    echo ""
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
    echo -e "${BOLD}${BLUE}"
    echo "  ╔═══════════════════════════════════╗"
    echo "  ║       OpenPylot Installer         ║"
    echo "  ╚═══════════════════════════════════╝"
    echo -e "${NC}"

    check_dependencies
    detect_platform
    resolve_version
    setup_directories
    install_binary
    configure_path
    run_init
    print_summary
}

main "$@"
