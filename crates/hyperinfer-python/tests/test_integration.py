"""Integration smoke tests for the core hyperinfer Python SDK.

These tests verify that the package structure is correct and that the
high-level Python API objects are importable and functional without
requiring a running Redis instance or compiled Rust extension.

Framework-specific integration tests live in their own packages:
  - bindings/hyperinfer-langchain/tests/
  - bindings/hyperinfer-llamaindex/tests/
"""

import pytest
from hyperinfer import Client, Config


@pytest.fixture
def config():
    return (
        Config()
        .with_api_key("openai", "sk-test")
        .with_alias("fast", "gpt-4o-mini")
        .with_default_provider("openai")
    )


def test_config_fluent_api(config):
    config = (
        Config()
        .with_api_key("openai", "sk-test")
        .with_api_key("anthropic", "sk-ant-test")
        .with_alias("smart", "gpt-4")
        .with_quota("team-a", max_requests_per_minute=100)
    )

    d = config.to_dict()
    assert "openai" in d["api_keys"]
    assert "anthropic" in d["api_keys"]
    assert d["model_aliases"]["smart"] == "gpt-4"


def test_client_accepts_config(config):
    """Client constructor must accept a Config without raising."""
    client = Client(redis_url="redis://localhost:6379", config=config)
    assert client is not None
    assert not client._initialized


def test_client_default_no_config():
    """Client can be created with no config (empty defaults)."""
    client = Client()
    assert client is not None
    assert not client._initialized


def test_config_to_dict_completeness(config):
    """to_dict() must contain all expected top-level keys."""
    d = config.to_dict()
    for key in (
        "api_keys",
        "routing_rules",
        "quotas",
        "model_aliases",
        "default_provider",
    ):
        assert key in d, f"Missing key in config dict: {key}"


def test_config_default_provider_preserved():
    cfg = Config().with_default_provider("anthropic")
    assert cfg.to_dict()["default_provider"] == "anthropic"


def test_config_quota_round_trip():
    cfg = Config().with_quota("k", rpm=30, tpm=5000, budget_cents=100)
    q = cfg.to_dict()["quotas"]["k"]
    assert q["max_requests_per_minute"] == 30
    assert q["max_tokens_per_minute"] == 5000
    assert q["budget_cents"] == 100
