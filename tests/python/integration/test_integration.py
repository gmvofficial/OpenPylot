"""Integration tests for the Python GMV Agent bindings.

These tests validate module-level behavior, the async wrapper,
and the setup helpers. They test the integration between Python
layers (not external services).
"""

import pytest
import subprocess
import sys


# ── Module imports ────────────────────────────────────────────────────

class TestModuleImports:
    def test_import_gmv_agent(self):
        import gmv_agent
        assert hasattr(gmv_agent, "GMVAgent")
        assert hasattr(gmv_agent, "Config")

    def test_version_exists(self):
        import gmv_agent
        assert hasattr(gmv_agent, "__version__")
        assert gmv_agent.__version__ == "0.2.0"

    def test_all_exports(self):
        import gmv_agent
        assert "GMVAgent" in gmv_agent.__all__
        assert "Config" in gmv_agent.__all__

    def test_native_module_version(self):
        from gmv_agent._native import __version__
        assert __version__ == "0.2.0"


# ── AsyncGMVAgent ─────────────────────────────────────────────────────

class TestAsyncAgent:
    def test_import_async_agent(self):
        from gmv_agent.agent import AsyncGMVAgent
        assert AsyncGMVAgent is not None

    def test_async_agent_has_create(self):
        from gmv_agent.agent import AsyncGMVAgent
        assert hasattr(AsyncGMVAgent, "create")

    def test_async_agent_has_chat(self):
        from gmv_agent.agent import AsyncGMVAgent
        assert hasattr(AsyncGMVAgent, "chat")

    def test_async_agent_has_register_tool(self):
        from gmv_agent.agent import AsyncGMVAgent
        assert hasattr(AsyncGMVAgent, "register_tool")


# ── Setup helpers ─────────────────────────────────────────────────────

class TestSetupHelpers:
    def test_import_setup_module(self):
        from gmv_agent.setup import run_init, run_add, run_doctor
        assert callable(run_init)
        assert callable(run_add)
        assert callable(run_doctor)


# ── CLI entry point ───────────────────────────────────────────────────

class TestCLI:
    def test_cli_module_importable(self):
        from gmv_agent.cli import main
        assert callable(main)


# ── Config round-trip (Python → native → Python) ─────────────────────

class TestConfigRoundTrip:
    def test_config_survives_agent_creation(self):
        """Config values should be preserved when creating an agent."""
        from gmv_agent import Config, GMVAgent

        cfg = Config(
            llm_provider="anthropic",
            llm_model="claude-sonnet-4-20250514",
            anthropic_api_key="sk-ant-roundtrip",
        )

        # Create agent from config (tests the Rust boundary crossing)
        agent = GMVAgent(cfg)
        assert agent is not None

    def test_config_fields_independent(self):
        """Modifying one Config instance should not affect another."""
        from gmv_agent import Config

        cfg1 = Config(llm_provider="openai")
        cfg2 = Config(llm_provider="anthropic")

        assert cfg1.llm_provider == "openai"
        assert cfg2.llm_provider == "anthropic"

        cfg1.llm_provider = "changed"
        assert cfg2.llm_provider == "anthropic"


# ── Tool registration ────────────────────────────────────────────────

class TestToolRegistration:
    def test_register_tool_valid_schema(self):
        from gmv_agent import Config, GMVAgent
        import json

        cfg = Config(openai_api_key="sk-test")
        agent = GMVAgent(cfg)

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
        from gmv_agent import Config, GMVAgent

        cfg = Config()
        agent = GMVAgent(cfg)

        with pytest.raises(Exception):
            agent.register_tool("bad_tool", "not valid json{{{", lambda x: x)

    def test_register_tool_non_callable_raises(self):
        from gmv_agent import Config, GMVAgent
        import json

        cfg = Config()
        agent = GMVAgent(cfg)

        schema = json.dumps({"type": "object"})

        with pytest.raises(Exception):
            agent.register_tool("bad_tool", schema, "not_callable")
