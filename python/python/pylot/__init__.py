"""OpenPylot — A Rust-powered personal AI assistant.

Python bindings via PyO3 / maturin.

Quick Start:
    >>> from pylot import PylotAgent
    >>> PylotAgent.init()          # Interactive setup wizard
    >>> agent = PylotAgent.from_config()
    >>> print(agent.chat("Hi!"))

Programmatic / CI:
    >>> from pylot import PylotAgent, Config
    >>> config = Config(llm_provider="openai", openai_api_key="sk-...")
    >>> agent = PylotAgent(config)
    >>> agent.chat("Schedule a meeting tomorrow at 3pm")
"""

from pylot._native import PylotAgent, Config, PylotMemory, PylotSkills, PylotLearning
from pylot._native import __version__

# Companion apps and MCP servers are server-side concepts — they only exist
# while `pylot serve` is running — so they are reached over HTTP rather than
# through the native bindings. Imported lazily so the package still works when
# no server is up.
from pylot.companions import Companions, CompanionError

__all__ = [
    "PylotAgent",
    "Config",
    "PylotMemory",
    "PylotSkills",
    "PylotLearning",
    "Companions",
    "CompanionError",
]
