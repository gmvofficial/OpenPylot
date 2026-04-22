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

__all__ = ["PylotAgent", "Config", "PylotMemory", "PylotSkills", "PylotLearning"]
__version__ = "0.3.0"
