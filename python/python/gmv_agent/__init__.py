"""GMV Agent — A Rust-powered personal AI assistant.

Python bindings via PyO3 / maturin.

Quick Start:
    >>> from gmv_agent import GMVAgent
    >>> GMVAgent.init()          # Interactive setup wizard
    >>> agent = GMVAgent.from_config()
    >>> print(agent.chat("Hi!"))

Programmatic / CI:
    >>> from gmv_agent import GMVAgent, Config
    >>> config = Config(llm_provider="openai", openai_api_key="sk-...")
    >>> agent = GMVAgent(config)
    >>> agent.chat("Schedule a meeting tomorrow at 3pm")
"""

from gmv_agent._native import GMVAgent, Config

__all__ = ["GMVAgent", "Config"]
__version__ = "0.2.0"
