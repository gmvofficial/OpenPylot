#!/usr/bin/env bash
# Auto-bootstrapping wrapper for fetch_transcript.py
# Creates a venv and installs deps on first run — works for any user.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SKILL_DIR="$(dirname "$SCRIPT_DIR")"
VENV_DIR="$SKILL_DIR/.venv"
PYTHON="$VENV_DIR/bin/python3"

# Auto-create venv + install deps if missing
if [ ! -f "$PYTHON" ]; then
    echo '{"status":"installing","message":"Setting up YouTube transcript tool (first run)..."}' >&2
    python3 -m venv "$VENV_DIR" 2>/dev/null || python -m venv "$VENV_DIR" 2>/dev/null
    "$VENV_DIR/bin/pip" install -q youtube-transcript-api requests 2>/dev/null
fi

# Run the actual script with all arguments forwarded
exec "$PYTHON" "$SCRIPT_DIR/fetch_transcript.py" "$@"
