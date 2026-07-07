"""CLI entry point for the Python package.

Registered as a console_script in pyproject.toml so that
``pip install openpylot`` makes ``pylot`` available on PATH.

This thin wrapper delegates to the native ``pylot`` binary for all heavy
lifting. Because the console script is itself named ``pylot``, we take care to
locate the *real* binary and never re-invoke this wrapper (which would fork-bomb
into ``BlockingIOError``).
"""

from __future__ import annotations

import os
import subprocess
import sys

_INSTALL_HELP = (
    "Error: the native `pylot` binary was not found on your PATH.\n"
    "The Python package wraps the Rust binary — install it with one of:\n"
    "  curl -fsSL https://raw.githubusercontent.com/gmvofficial/OpenPylot/main/install.sh | bash\n"
    "  cargo install openpylot\n"
)


def _find_binary() -> str | None:
    """Return the path to the real pylot binary, skipping this wrapper."""
    self_path = os.path.realpath(sys.argv[0]) if sys.argv and sys.argv[0] else ""

    def usable(path: str) -> bool:
        return (
            os.path.isfile(path)
            and os.access(path, os.X_OK)
            and os.path.realpath(path) != self_path
        )

    # 1. Anything named `pylot` on PATH that isn't this console script.
    for directory in os.environ.get("PATH", "").split(os.pathsep):
        if not directory:
            continue
        candidate = os.path.join(directory, "pylot")
        if usable(candidate):
            return candidate

    # 2. Common install locations used by the installer / cargo / Homebrew.
    for candidate in (
        os.path.expanduser("~/.pylot/bin/pylot"),
        os.path.expanduser("~/.cargo/bin/pylot"),
        "/usr/local/bin/pylot",
        "/opt/homebrew/bin/pylot",
    ):
        if usable(candidate):
            return candidate

    return None


def main() -> None:
    """Forward all arguments to the native pylot binary."""
    # Handle `--version` from the compiled module so it works even without the
    # standalone binary installed.
    if sys.argv[1:] in (["--version"], ["-V"]):
        try:
            from pylot._native import __version__

            print(f"pylot {__version__}")
            sys.exit(0)
        except Exception:  # pragma: no cover - fall through to the binary
            pass

    binary = _find_binary()
    if binary is None:
        print(_INSTALL_HELP, file=sys.stderr)
        sys.exit(1)

    try:
        result = subprocess.run([binary] + sys.argv[1:], check=False)
        sys.exit(result.returncode)
    except FileNotFoundError:
        print(_INSTALL_HELP, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
