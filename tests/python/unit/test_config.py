"""Smoke tests for the Python bindings.

These tests validate the Python-side wrappers.  They do NOT require a
running LLM or external service — they only verify that the module
loads, Config constructs correctly, etc.
"""

import pytest
from gmv_agent import Config


def test_config_defaults():
    cfg = Config()
    assert cfg.llm_provider == "openai"
    assert cfg.llm_model == "gpt-4o"
    assert cfg.openai_api_key is None


def test_config_custom():
    cfg = Config(
        llm_provider="anthropic",
        llm_model="claude-sonnet-4-20250514",
        anthropic_api_key="sk-ant-test",
    )
    assert cfg.llm_provider == "anthropic"
    assert cfg.anthropic_api_key == "sk-ant-test"


def test_config_repr():
    cfg = Config(openai_api_key="sk-test")
    r = repr(cfg)
    assert "openai" in r
    assert "sk-test" not in r  # key should be masked
    assert "***" in r


def test_config_setters():
    cfg = Config()
    cfg.llm_provider = "anthropic"
    cfg.llm_model = "claude-opus-4-20250514"
    assert cfg.llm_provider == "anthropic"
    assert cfg.llm_model == "claude-opus-4-20250514"
