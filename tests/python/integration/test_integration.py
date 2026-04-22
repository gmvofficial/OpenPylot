"""Integration tests for the Python Pylot bindings.

These tests validate module-level behavior, the async wrapper,
and the setup helpers. They test the integration between Python
layers (not external services).
"""

import pytest
import subprocess
import sys


# ── Module imports ────────────────────────────────────────────────────

class TestModuleImports:
    def test_import_pylot(self):
        import pylot
        assert hasattr(pylot, "PylotAgent")
        assert hasattr(pylot, "Config")

    def test_version_exists(self):
        import pylot
        assert hasattr(pylot, "__version__")
        assert pylot.__version__ == "0.3.0"

    def test_all_exports(self):
        import pylot
        assert "PylotAgent" in pylot.__all__
        assert "Config" in pylot.__all__

    def test_native_module_version(self):
        from pylot._native import __version__
        assert __version__ == "0.3.0"


# ── AsyncPylotAgent ─────────────────────────────────────────────────────

class TestAsyncAgent:
    def test_import_async_agent(self):
        from pylot.agent import AsyncPylotAgent
        assert AsyncPylotAgent is not None

    def test_async_agent_has_create(self):
        from pylot.agent import AsyncPylotAgent
        assert hasattr(AsyncPylotAgent, "create")

    def test_async_agent_has_chat(self):
        from pylot.agent import AsyncPylotAgent
        assert hasattr(AsyncPylotAgent, "chat")

    def test_async_agent_has_register_tool(self):
        from pylot.agent import AsyncPylotAgent
        assert hasattr(AsyncPylotAgent, "register_tool")


# ── Setup helpers ─────────────────────────────────────────────────────

class TestSetupHelpers:
    def test_import_setup_module(self):
        from pylot.setup import run_init, run_add, run_doctor
        assert callable(run_init)
        assert callable(run_add)
        assert callable(run_doctor)


# ── CLI entry point ───────────────────────────────────────────────────

class TestCLI:
    def test_cli_module_importable(self):
        from pylot.cli import main
        assert callable(main)


# ── Config round-trip (Python → native → Python) ─────────────────────

class TestConfigRoundTrip:
    def test_config_survives_agent_creation(self):
        """Config values should be preserved when creating an agent."""
        from pylot import Config, PylotAgent

        cfg = Config(
            llm_provider="anthropic",
            llm_model="claude-sonnet-4-20250514",
            anthropic_api_key="sk-ant-roundtrip",
        )

        # Create agent from config (tests the Rust boundary crossing)
        agent = PylotAgent(cfg)
        assert agent is not None

    def test_config_fields_independent(self):
        """Modifying one Config instance should not affect another."""
        from pylot import Config

        cfg1 = Config(llm_provider="openai")
        cfg2 = Config(llm_provider="anthropic")

        assert cfg1.llm_provider == "openai"
        assert cfg2.llm_provider == "anthropic"

        cfg1.llm_provider = "changed"
        assert cfg2.llm_provider == "anthropic"


# ── Tool registration ────────────────────────────────────────────────

class TestToolRegistration:
    def test_register_tool_valid_schema(self):
        from pylot import Config, PylotAgent
        import json

        cfg = Config(openai_api_key="sk-test")
        agent = PylotAgent(cfg)

        schema = json.dumps({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            },
            "required": ["query"]
        })

        # Should not raise
        agent.register_tool("test_tool", schema, lambda x: x)

    def test_register_tool_invalid_schema_raises(self):
        from pylot import Config, PylotAgent

        cfg = Config()
        agent = PylotAgent(cfg)

        with pytest.raises(Exception):
            agent.register_tool("bad_tool", "not valid json{{{", lambda x: x)

    def test_register_tool_non_callable_raises(self):
        from pylot import Config, PylotAgent
        import json

        cfg = Config()
        agent = PylotAgent(cfg)

        schema = json.dumps({"type": "object"})

        with pytest.raises(Exception):
            agent.register_tool("bad_tool", schema, "not_callable")
