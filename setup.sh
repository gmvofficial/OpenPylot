#!/usr/bin/env bash
set -euo pipefail

# ─── GMV Agent Setup Script ──────────────────────────────────────────
# Quick setup for the GMV Agent personal AI assistant.
# Usage: ./setup.sh

BOLD='\033[1m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${BLUE}${BOLD}"
echo "  ╔══════════════════════════════════════════════════╗"
echo "  ║      🤖 GMV Agent — Setup Script                ║"
echo "  ║      Personal AI Assistant (Rust)                ║"
echo "  ╚══════════════════════════════════════════════════╝"
echo -e "${NC}"

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo -e "${YELLOW}Rust not found. Installing via rustup...${NC}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo -e "${GREEN}✅ Rust installed.${NC}"
else
    echo -e "${GREEN}✅ Rust found: $(rustc --version)${NC}"
fi

# Create .env if it doesn't exist
if [ ! -f .env ]; then
    if [ -f .env.example ]; then
        cp .env.example .env
        echo -e "${GREEN}✅ Created .env from .env.example${NC}"
    else
        echo -e "${YELLOW}⚠ No .env.example found. Creating minimal .env...${NC}"
        cat > .env << 'EOF'
# GMV Agent Configuration
# Add at least one LLM provider API key:
OPENAI_API_KEY=
ANTHROPIC_API_KEY=
LLM_PROVIDER=openai
EOF
    fi
    echo -e "${YELLOW}📝 Edit .env to add your API key(s) before running the agent.${NC}"
else
    echo -e "${GREEN}✅ .env already exists${NC}"
fi

# Create data directory
DATA_DIR="$HOME/.gmv-agent/data"
mkdir -p "$DATA_DIR"
echo -e "${GREEN}✅ Data directory ready: ${DATA_DIR}${NC}"

# Build the project
echo -e "\n${BLUE}${BOLD}Building GMV Agent...${NC}"
cargo build --release 2>&1

if [ $? -eq 0 ]; then
    echo -e "\n${GREEN}${BOLD}✅ Build successful!${NC}"
    echo -e "\n${BOLD}Next steps:${NC}"
    echo -e "  1. Edit ${YELLOW}.env${NC} and add your API key(s)"
    echo -e "  2. Run the agent: ${GREEN}cargo run --release${NC}"
    echo -e "  3. (Optional) Set up Google Calendar: ${GREEN}cargo run --release -- setup google-calendar${NC}"
    echo -e ""
    echo -e "  Or use the binary directly: ${GREEN}./target/release/gmv-agent${NC}"
else
    echo -e "\n${RED}${BOLD}❌ Build failed. Check the error messages above.${NC}"
    exit 1
fi
