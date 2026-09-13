"""Companion apps and MCP servers, over a running ``pylot serve``.

These are server-side concepts: a companion is a child process supervised by
the server and reached through its authenticated proxy, and an MCP server's
connection state only exists while the agent is up. So this talks to a running
instance over HTTP rather than going through the native bindings — there is no
meaningful offline answer to "is dbpylot running".

The access token is read from the same file the server writes, so nothing needs
configuring::

    from pylot.companions import Companions

    c = Companions()
    for app in c.list():
        print(app["name"], app["state"])

    url = c.start("dbpylot")["url"]     # mount path, e.g. /companions/dbpylot/
    print(c.embed_url("dbpylot"))       # a full URL an iframe can load
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

__all__ = ["Companions", "CompanionError", "default_token", "DEFAULT_BASE_URL"]

DEFAULT_BASE_URL = "http://127.0.0.1:3001"

#: Where ``pylot serve`` writes its per-install access token.
TOKEN_PATH = Path.home() / ".pylot" / "data" / "api-token"


class CompanionError(RuntimeError):
    """The server refused or could not satisfy the request."""

    def __init__(self, message: str, status: int | None = None) -> None:
        super().__init__(message)
        self.status = status


def default_token() -> str | None:
    """The access token for this install, or ``None`` if there is not one yet.

    Checked in the order a caller would expect: an explicit environment
    variable first, then the file the server writes on first run.
    """
    from_env = os.environ.get("PYLOT_API_TOKEN")
    if from_env:
        return from_env
    try:
        token = TOKEN_PATH.read_text().strip()
        return token or None
    except OSError:
        return None


class Companions:
    """Client for the companion and MCP endpoints of a running server."""

    def __init__(
        self,
        base_url: str = DEFAULT_BASE_URL,
        token: str | None = None,
        timeout: float = 30.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token or default_token()
        self.timeout = timeout

    # ── Companions ───────────────────────────────────────────────────

    def list(self) -> list[dict[str, Any]]:
        """Every companion, with whether it is installed, stopped or running."""
        return self._request("GET", "/api/companions")

    def start(self, name: str) -> dict[str, Any]:
        """Start a companion and return ``{name, port, url}``.

        Starting one that is already running is a no-op that returns the same
        URL, so this is safe to call unconditionally.
        """
        return self._request("POST", f"/api/companions/{urllib.parse.quote(name)}/start")

    def stop(self, name: str) -> bool:
        """Stop a companion. Stopping one that is not running is a no-op."""
        return self._request("POST", f"/api/companions/{urllib.parse.quote(name)}/stop")

    def embed_url(self, name: str, start: bool = True) -> str:
        """A full URL for a companion's own web interface.

        Suitable for an iframe or a browser. The token is included as a query
        parameter because that is the only channel available to an ``<iframe
        src>`` — it cannot send a header.
        """
        if start:
            self.start(name)
        path = f"/companions/{urllib.parse.quote(name)}/"
        if not self.token:
            return f"{self.base_url}{path}"
        return f"{self.base_url}{path}?token={urllib.parse.quote(self.token)}"

    # ── MCP servers ──────────────────────────────────────────────────

    def mcp_servers(self) -> list[dict[str, Any]]:
        """Every configured MCP server, including disabled and failing ones."""
        return self._request("GET", "/api/mcp/config")

    def add_mcp_server(
        self,
        name: str,
        command: str | None = None,
        args: list[str] | None = None,
        url: str | None = None,
        env: dict[str, str] | None = None,
    ) -> dict[str, Any]:
        """Add or update an MCP server. Takes effect on the next restart."""
        if not command and not url:
            raise ValueError("provide either command (stdio) or url (http/sse)")
        body: dict[str, Any] = {"name": name}
        if command:
            body["command"] = command
        if args:
            body["args"] = args
        if url:
            body["url"] = url
        if env:
            body["env"] = env
        return self._request("POST", "/api/mcp/config", body)

    def remove_mcp_server(self, name: str) -> bool:
        return self._request("DELETE", f"/api/mcp/config/{urllib.parse.quote(name)}")

    def set_mcp_server_enabled(self, name: str, enabled: bool) -> bool:
        return self._request(
            "PATCH", f"/api/mcp/config/{urllib.parse.quote(name)}", {"enabled": enabled}
        )

    def test_mcp_server(self, name: str) -> dict[str, Any]:
        """Connect to one server and report what answered.

        Uses a throwaway connection, so this never disturbs the live registry.
        """
        return self._request("POST", f"/api/mcp/config/{urllib.parse.quote(name)}/test")

    # ── Transport ────────────────────────────────────────────────────

    def _request(self, method: str, path: str, body: Any = None) -> Any:
        url = f"{self.base_url}{path}"
        data = json.dumps(body).encode() if body is not None else None

        request = urllib.request.Request(url, data=data, method=method)
        request.add_header("Content-Type", "application/json")
        if self.token:
            request.add_header("Authorization", f"Bearer {self.token}")

        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                payload = json.loads(response.read().decode() or "{}")
        except urllib.error.HTTPError as e:
            detail = _error_detail(e)
            if e.code == 401:
                raise CompanionError(
                    "The server rejected the access token. Run 'pylot token' and pass it "
                    "as token=..., or set PYLOT_API_TOKEN.",
                    status=401,
                ) from e
            raise CompanionError(detail or f"HTTP {e.code}", status=e.code) from e
        except urllib.error.URLError as e:
            raise CompanionError(
                f"Could not reach OpenPylot at {self.base_url}. Is 'pylot serve' running? ({e.reason})"
            ) from e

        # The server wraps every response as {success, data}.
        if isinstance(payload, dict) and not payload.get("success", True):
            raise CompanionError(payload.get("error", "request failed"))
        if isinstance(payload, dict) and "data" in payload:
            return payload["data"]
        return payload


def _error_detail(error: urllib.error.HTTPError) -> str | None:
    """Pull the server's own message out of an error response, if it sent one."""
    try:
        body = json.loads(error.read().decode())
    except Exception:
        return None
    if isinstance(body, dict):
        return body.get("error")
    return None
