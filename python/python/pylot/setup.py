"""Setup wizard helpers (Python-side).

The actual wizard logic lives in the Rust binary (``pylot init``).
This module exists so Python callers can programmatically trigger
specific setup steps if needed.
"""

from __future__ import annotations

import subprocess
import sys
from typing import Optional


def run_init(*, reset: bool = False, only: Optional[str] = None) -> int:
    """Launch the interactive init wizard.

    Args:
        reset: If True, clear all config and start fresh (``--reset``).
        only:  Set up only a single service (e.g., ``"google-calendar"``).

    Returns:
        Process exit code (0 on success).
    """
    cmd = ["pylot", "init"]
    if reset:
        cmd.append("--reset")
    if only:
        cmd.extend(["--only", only])

    result = subprocess.run(cmd, check=False)
    return result.returncode


def run_add(service: str) -> int:
    """Add a single integration.

    Equivalent to ``pylot add <service>``.
    """
    result = subprocess.run(["pylot", "add", service], check=False)
    return result.returncode


def run_doctor() -> int:
    """Run diagnostic checks."""
    result = subprocess.run(["pylot", "doctor"], check=False)
    return result.returncode


if __name__ == "__main__":
    sys.exit(run_init())
