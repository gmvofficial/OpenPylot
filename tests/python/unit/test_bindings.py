"""Unit tests for the Python GMV Agent bindings.

Tests Config construction, defaults, property setters, repr masking, and
GMVAgent instantiation from Config. These tests do NOT require a running
LLM or external service — they only verify the binding layer.
"""

import pytest
from gmv_agent import Config, GMVAgent


# ── Config defaults ───────────────────────────────────────────────────

class TestConfigDefaults:
    def test_default_provider(self):
        cfg = Config()
        assert cfg.llm_provider == "openai"

    def test_default_model(self):
        cfg = Config()
        assert cfg.llm_model == "gpt-4o"

    def test_default_keys_are_none(self):
        cfg = Config()
        assert cfg.openai_api_key is None
        assert cfg.anthropic_api_key is None
        assert cfg.google_credentials_file is None
        assert cfg.telegram_bot_token is None
        assert cfg.telegram_chat_id is None


# ── Config custom values ──────────────────────────────────────────────

class TestConfigCustom:
    def test_anthropic_config(self):
        cfg = Config(
            llm_provider="anthropic",
            llm_model="claude-sonnet-4-20250514",
            anthropic_api_key="sk-ant-test",
        )
        assert cfg.llm_provider == "anthropic"
        assert cfg.llm_model == "claude-sonnet-4-20250514"
        assert cfg.anthropic_api_key == "sk-ant-test"

    def test_openai_with_key(self):
        cfg = Config(openai_api_key="sk-openai-test")
        assert cfg.openai_api_key == "sk-openai-test"
        assert cfg.llm_provider == "openai"

    def test_all_fields(self):
        cfg = Config(
            llm_provider="openai",
            llm_model="gpt-4o-mini",
            openai_api_key="sk-openai",
            anthropic_api_key="sk-ant",
            google_credentials_file="/tmp/creds.json",
            telegram_bot_token="123:ABC",
            telegram_chat_id="-100123",
        )
        assert cfg.google_credentials_file == "/tmp/creds.json"
        assert cfg.telegram_bot_token == "123:ABC"
        assert cfg.telegram_chat_id == "-100123"


# ── Config property setters ───────────────────────────────────────────

class TestConfigSetters:
    def test_set_provider(self):
        cfg = Config()
        cfg.llm_provider = "anthropic"
        assert cfg.llm_provider == "anthropic"

    def test_set_model(self):
        cfg = Config()
        cfg.llm_model = "claude-opus-4-20250514"
        assert cfg.llm_model == "claude-opus-4-20250514"

    def test_set_api_key(self):
        cfg = Config()
        cfg.openai_api_key = "sk-new-key"
        assert cfg.openai_api_key == "sk-new-key"

    def test_set_telegram_fields(self):
        cfg = Config()
        cfg.telegram_bot_token = "bot:token123"
        cfg.telegram_chat_id = "-999"
        assert cfg.telegram_bot_token == "bot:token123"
        assert cfg.telegram_chat_id == "-999"


# ── Config repr ───────────────────────────────────────────────────────

class TestConfigRepr:
    def test_repr_contains_provider(self):
        cfg = Config()
        r = repr(cfg)
        assert "openai" in r

    def test_repr_masks_openai_key(self):
        cfg = Config(openai_api_key="sk-super-secret")
        r = repr(cfg)
        assert "sk-super-secret" not in r
        assert "***" in r

    def test_repr_masks_anthropic_key(self):
        cfg = Config(anthropic_api_key="sk-ant-secret")
        r = repr(cfg)
        assert "sk-ant-secret" not in r
        assert "***" in r

    def test_repr_shows_none_for_missing_keys(self):
        cfg = Config()
        r = repr(cfg)
        assert "None" in r


# ── GMVAgent from Config ──────────────────────────────────────────────

class TestGMVAgentFromConfig:
    def test_create_agent_from_default_config(self):
        cfg = Config()
        agent = GMVAgent(cfg)
        assert agent is not None

    def test_create_agent_from_custom_config(self):
        cfg = Config(
            llm_provider="anthropic",
            llm_model="claude-sonnet-4-20250514",
            anthropic_api_key="sk-ant-test",
        )
        agent = GMVAgent(cfg)
        assert agent is not None

    def test_from_config_missing_file_raises(self):
        with pytest.raises(Exception):
            GMVAgent.from_config("/nonexistent/path/secrets.enc")
