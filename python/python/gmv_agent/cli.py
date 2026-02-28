"""CLI entry point for the Python package.

Registered as a console_script in pyproject.toml so that
``pip install gmv-agent`` makes ``gmv-agent`` available on PATH.

This thin wrapper delegates to the Rust binary for all heavy lifting.
"""

from __future__ import annotations

import subprocess
import sys


def main() -> None:
    """Forward all arguments to the native gmv-agent binary."""
    try:
        result = subprocess.run(
            ["gmv-agent"] + sys.argv[1:],
            check=False,
        )
        sys.exit(result.returncode)
    except FileNotFoundError:
        print(
            "Error: gmv-agent binary not found on PATH.\n"
            "Install it first:\n"
            "  curl -fsSL https://get.gmvagent.com/install.sh | sh\n"
            "  # or: cargo install gmv-agent",
            file=sys.stderr,
        )
        sys.exit(1)


if __name__ == "__main__":
    main()
