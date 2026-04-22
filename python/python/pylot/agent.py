"""High-level Python wrapper around the native Rust bindings.

Provides async helpers and convenience methods on top of the native
PylotAgent class.  Import from the top-level package instead:

    from pylot import PylotAgent
"""

from __future__ import annotations

import asyncio
from typing import Any, Callable, Optional

from pylot._native import PylotAgent as _NativeAgent, Config


class AsyncPylotAgent:
    """Async-friendly wrapper for the Pylot agent.

    Usage::

        agent = await AsyncPylotAgent.create()
        response = await agent.chat("Hello!")
    """

    def __init__(self, native: _NativeAgent) -> None:
        self._native = native

    @classmethod
    async def create(cls, config: Optional[Config] = None) -> "AsyncPylotAgent":
        """Create an agent, optionally from a Config object."""
        loop = asyncio.get_event_loop()
        if config is not None:
            native = await loop.run_in_executor(None, _NativeAgent, config)
        else:
            native = await loop.run_in_executor(
                None, _NativeAgent.from_config, "~/.pylot/secrets.enc"
            )
        return cls(native)

    async def chat(self, message: str) -> str:
        """Send a message and return the agent's response."""
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(None, self._native.chat, message)

    def register_tool(
        self, name: str, schema: str, callback: Callable[..., Any]
    ) -> None:
        """Register a custom tool callback."""
        self._native.register_tool(name, schema, callback)
