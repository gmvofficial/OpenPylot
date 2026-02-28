"""High-level Python wrapper around the native Rust bindings.

Provides async helpers and convenience methods on top of the native
GMVAgent class.  Import from the top-level package instead:

    from gmv_agent import GMVAgent
"""

from __future__ import annotations

import asyncio
from typing import Any, Callable, Optional

from gmv_agent._native import GMVAgent as _NativeAgent, Config


class AsyncGMVAgent:
    """Async-friendly wrapper for the GMV Agent.

    Usage::

        agent = await AsyncGMVAgent.create()
        response = await agent.chat("Hello!")
    """

    def __init__(self, native: _NativeAgent) -> None:
        self._native = native

    @classmethod
    async def create(cls, config: Optional[Config] = None) -> "AsyncGMVAgent":
        """Create an agent, optionally from a Config object."""
        loop = asyncio.get_event_loop()
        if config is not None:
            native = await loop.run_in_executor(None, _NativeAgent, config)
        else:
            native = await loop.run_in_executor(
                None, _NativeAgent.from_config, "~/.gmv-agent/secrets.enc"
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
