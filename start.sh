#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# start.sh — Launch the OpenPylot backend (Rust) and frontend (Next.js) together.
#
# Usage:
#   ./start.sh              # dev mode (default): hot-reload frontend on :3000
#   ./start.sh prod         # build frontend statically + serve via backend :3001
#
# In dev mode open:    http://localhost:3000
# In prod mode open:   http://localhost:3001
#
# Press Ctrl+C once to gracefully stop both processes.
# ─────────────────────────────────────────────────────────────────────────────

set -euo pipefail

# Resolve the script's own directory so the script works no matter where it's
# called from (cron, an alias, a different cwd, etc.).
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

MODE="${1:-dev}"
BACKEND_PORT=3001
FRONTEND_PORT=3000

# ── Colour helpers ───────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
    C_RESET='\033[0m'; C_BOLD='\033[1m'
    C_BLUE='\033[34m'; C_GREEN='\033[32m'; C_YELLOW='\033[33m'; C_RED='\033[31m'
else
    C_RESET=''; C_BOLD=''; C_BLUE=''; C_GREEN=''; C_YELLOW=''; C_RED=''
fi
log()  { printf "${C_BLUE}[start.sh]${C_RESET} %s\n"  "$*"; }
ok()   { printf "${C_GREEN}[start.sh]${C_RESET} %s\n" "$*"; }
warn() { printf "${C_YELLOW}[start.sh]${C_RESET} %s\n" "$*"; }
die()  { printf "${C_RED}[start.sh]${C_RESET} %s\n" "$*" >&2; exit 1; }

# ── Dependency checks ────────────────────────────────────────────────────────
command -v cargo >/dev/null 2>&1 || die "Rust 'cargo' not found. Install: https://rustup.rs"
command -v npm   >/dev/null 2>&1 || die "Node 'npm' not found. Install Node 18+ from https://nodejs.org"

# ── Free up ports if something is already listening ─────────────────────────
free_port() {
    local port=$1
    local pids
    pids=$(lsof -ti :"$port" 2>/dev/null || true)
    if [[ -n "$pids" ]]; then
        warn "Port $port busy — killing PID(s): $pids"
        # shellcheck disable=SC2086
        kill -9 $pids 2>/dev/null || true
        sleep 1
    fi
}

# ── Kill any leftover pylot/next processes from previous runs ───────────────
# This is more reliable than free_port alone because sometimes a stale process
# binds to a port that lsof momentarily can't see (e.g. during TIME_WAIT).
kill_stale_processes() {
    pkill -9 -f "target/debug/pylot serve"   2>/dev/null || true
    pkill -9 -f "target/release/pylot serve" 2>/dev/null || true
    pkill -9 -f "next dev"                   2>/dev/null || true
    pkill -9 -f "next-server"                2>/dev/null || true
    sleep 1
}

# ── Open URL in the user's default browser (cross-platform) ─────────────────
open_browser() {
    local url=$1
    if command -v open >/dev/null 2>&1; then
        open "$url" 2>/dev/null &   # macOS
    elif command -v xdg-open >/dev/null 2>&1; then
        xdg-open "$url" 2>/dev/null &  # Linux
    elif command -v start >/dev/null 2>&1; then
        start "$url" 2>/dev/null &   # Windows (Git Bash / WSL)
    else
        warn "Couldn't auto-open browser. Please open $url manually."
    fi
}

# ── Install frontend deps on first run (or repair a broken install) ─────────
# Check for the `next` binary, not just the node_modules dir — an interrupted
# npm install (e.g. disk full) can leave node_modules present but incomplete.
if [[ ! -x frontend/node_modules/.bin/next ]]; then
    log "Installing frontend dependencies…"
    (cd frontend && rm -rf node_modules && npm install)
fi

# ── Track child PIDs so Ctrl+C kills both ───────────────────────────────────
BACKEND_PID=""
FRONTEND_PID=""

cleanup() {
    echo
    log "Shutting down…"
    [[ -n "$BACKEND_PID"  ]] && kill "$BACKEND_PID"  2>/dev/null || true
    [[ -n "$FRONTEND_PID" ]] && kill "$FRONTEND_PID" 2>/dev/null || true
    # Give them a moment, then force-kill anything still alive
    sleep 1
    [[ -n "$BACKEND_PID"  ]] && kill -9 "$BACKEND_PID"  2>/dev/null || true
    [[ -n "$FRONTEND_PID" ]] && kill -9 "$FRONTEND_PID" 2>/dev/null || true
    ok "Goodbye!"
    exit 0
}
trap cleanup INT TERM

# ─────────────────────────────────────────────────────────────────────────────
# Mode: prod — build static frontend, serve from Rust backend on :3001
# ─────────────────────────────────────────────────────────────────────────────
if [[ "$MODE" == "prod" ]]; then
    log "Building frontend (static export → frontend/out)…"
    (cd frontend && npm run build)

    kill_stale_processes
    free_port "$BACKEND_PORT"

    log "Starting backend on http://localhost:${BACKEND_PORT}"
    # stdin from /dev/null: the backend must not prompt for API keys here —
    # missing keys are set up from the frontend wizard instead.
    cargo run --release -- serve --foreground < /dev/null &
    BACKEND_PID=$!

    log "Waiting for backend to become ready…"
    for i in $(seq 1 180); do
        if curl -s -o /dev/null -m 1 "http://localhost:${BACKEND_PORT}/api/status"; then
            ok "Backend is up after ${i}s."
            break
        fi
        if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
            die "Backend process exited unexpectedly."
        fi
        sleep 1
    done

    echo
    ok "All set."
    printf "    Open this URL in your browser:\n"
    printf "    ${C_BOLD}http://localhost:${BACKEND_PORT}${C_RESET}\n"
    echo
    log "Opening browser…"
    open_browser "http://localhost:${BACKEND_PORT}"
    log "Press Ctrl+C to stop."
    wait "$BACKEND_PID"
    exit 0
fi

# ─────────────────────────────────────────────────────────────────────────────
# Mode: dev (default) — backend on :3001, Next.js dev server on :3000
# Next.js proxies /api and /ws to the backend (see frontend/next.config.ts).
# ─────────────────────────────────────────────────────────────────────────────
kill_stale_processes
free_port "$BACKEND_PORT"
free_port "$FRONTEND_PORT"

log "Starting backend on http://localhost:${BACKEND_PORT}"
log "(first run compiles Rust — this can take 1–2 minutes…)"
# stdin from /dev/null: the backend must not prompt for API keys here —
# missing keys are set up from the frontend wizard instead.
cargo run -- serve --foreground < /dev/null &
BACKEND_PID=$!

# ── Wait until the backend is actually accepting connections ────────────────
# Polling avoids the ECONNREFUSED proxy errors you get from a fixed `sleep`
# when cargo is still compiling on the first run.
log "Waiting for backend to become ready…"
for i in $(seq 1 180); do  # up to 3 minutes
    if curl -s -o /dev/null -m 1 "http://localhost:${BACKEND_PORT}/api/status"; then
        ok "Backend is up after ${i}s."
        break
    fi
    # Bail out early if the cargo process died
    if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
        die "Backend process exited unexpectedly. Check the output above."
    fi
    sleep 1
    if [[ $i -eq 180 ]]; then
        die "Backend didn't respond within 3 minutes. Check the output above."
    fi
done

log "Starting frontend dev server on http://localhost:${FRONTEND_PORT}"
(cd frontend && npm run dev) &
FRONTEND_PID=$!

# Wait for the Next.js dev server to actually be ready (it takes 3–10s to
# compile its first route). Opening the browser before this point shows a
# blank page or a "page not found" error.
log "Waiting for frontend to become ready…"
for i in $(seq 1 60); do
    if curl -s -o /dev/null -m 1 "http://localhost:${FRONTEND_PORT}"; then
        ok "Frontend is up after ${i}s."
        break
    fi
    if ! kill -0 "$FRONTEND_PID" 2>/dev/null; then
        die "Frontend process exited unexpectedly. Check the output above."
    fi
    sleep 1
done

echo
ok "Everything is up!"
echo
printf "    Opening ${C_BOLD}http://localhost:${FRONTEND_PORT}${C_RESET} in your browser…\n"
echo
open_browser "http://localhost:${FRONTEND_PORT}"
log "Press Ctrl+C to stop both processes."

# Wait for either child to exit; if one dies, tear the other down too.
# NOTE: macOS ships bash 3.2 which has no `wait -n`, so we poll instead.
while kill -0 "$BACKEND_PID" 2>/dev/null && kill -0 "$FRONTEND_PID" 2>/dev/null; do
    sleep 2
done
warn "One of the processes exited — shutting the other down."
cleanup
