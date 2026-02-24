"""Tests for Config class."""

import pytest

from hyperinfer.config import Config


class TestConfig:
    """Test suite for Config class."""

    def test_init(self):
        """Test Config initialization."""
        config = Config()
        assert config._api_keys == {}
        assert config._routing_rules == []
        assert config._quotas == {}
        assert config._model_aliases == {}
        assert config._default_provider is None

    def test_with_api_key(self):
        """Test adding API key."""
        config = Config()
        result = config.with_api_key("openai", "sk-test-key")

        assert config._api_keys == {"openai": "sk-test-key"}
        assert result is config

    def test_with_api_key_multiple_providers(self):
        """Test adding multiple API keys."""
        config = Config()
        config.with_api_key("openai", "sk-openai")
        config.with_api_key("anthropic", "sk-anthropic")

        assert config._api_keys == {
            "openai": "sk-openai",
            "anthropic": "sk-anthropic",
        }

    def test_with_alias(self):
        """Test adding model alias."""
        config = Config()
        result = config.with_alias("gpt-4-turbo", "gpt-4-0125-preview")

        assert config._model_aliases == {"gpt-4-turbo": "gpt-4-0125-preview"}
        assert result is config

    def test_with_alias_multiple(self):
        """Test adding multiple aliases."""
        config = Config()
        config.with_alias("gpt-4-turbo", "gpt-4-0125-preview")
        config.with_alias("claude-sonnet", "claude-3-sonnet-20240229")

        assert config._model_aliases == {
            "gpt-4-turbo": "gpt-4-0125-preview",
            "claude-sonnet": "claude-3-sonnet-20240229",
        }

    def test_with_routing_rule(self):
        """Test adding routing rule."""
        config = Config()
        result = config.with_routing_rule("primary", 10, ["gpt-4", "gpt-3.5-turbo"])

        assert len(config._routing_rules) == 1
        rule = config._routing_rules[0]
        assert rule["name"] == "primary"
        assert rule["priority"] == 10
        assert rule["fallback_models"] == ["gpt-4", "gpt-3.5-turbo"]
        assert result is config

    def test_with_routing_rule_multiple(self):
        """Test adding multiple routing rules."""
        config = Config()
        config.with_routing_rule("primary", 10, ["gpt-4"])
        config.with_routing_rule("fallback", 5, ["gpt-3.5-turbo"])

        assert len(config._routing_rules) == 2
        assert config._routing_rules[0]["name"] == "primary"
        assert config._routing_rules[1]["name"] == "fallback"

    def test_with_quota_all_params(self):
        """Test adding quota with all parameters."""
        config = Config()
        result = config.with_quota("default", rpm=60, tpm=100000, budget_cents=1000)

        assert "default" in config._quotas
        quota = config._quotas["default"]
        assert quota["max_requests_per_minute"] == 60
        assert quota["max_tokens_per_minute"] == 100000
        assert quota["budget_cents"] == 1000
        assert result is config

    def test_with_quota_partial_params(self):
        """Test adding quota with partial parameters."""
        config = Config()
        config.with_quota("default", rpm=60)

        quota = config._quotas["default"]
        assert quota["max_requests_per_minute"] == 60
        assert quota["max_tokens_per_minute"] is None
        assert quota["budget_cents"] is None

    def test_with_quota_none_params(self):
        """Test adding quota with all None parameters."""
        config = Config()
        config.with_quota("default")

        quota = config._quotas["default"]
        assert quota["max_requests_per_minute"] is None
        assert quota["max_tokens_per_minute"] is None
        assert quota["budget_cents"] is None

    def test_with_default_provider(self):
        """Test setting default provider."""
        config = Config()
        result = config.with_default_provider("openai")

        assert config._default_provider == "openai"
        assert result is config

    def test_to_dict_empty(self):
        """Test to_dict with empty config."""
        config = Config()
        result = config.to_dict()

        assert result == {
            "api_keys": {},
            "routing_rules": [],
            "quotas": {},
            "model_aliases": {},
            "default_provider": None,
        }

    def test_to_dict_with_data(self):
        """Test to_dict with populated config."""
        config = Config()
        config.with_api_key("openai", "sk-test")
        config.with_alias("gpt-4-turbo", "gpt-4-0125-preview")
        config.with_routing_rule("primary", 10, ["gpt-4"])
        config.with_quota("default", rpm=60)
        config.with_default_provider("openai")

        result = config.to_dict()

        assert result["api_keys"] == {"openai": "sk-test"}
        assert result["model_aliases"] == {"gpt-4-turbo": "gpt-4-0125-preview"}
        assert len(result["routing_rules"]) == 1
        assert result["quotas"]["default"]["max_requests_per_minute"] == 60
        assert result["default_provider"] == "openai"

    def test_fluent_api_chain(self):
        """Test fluent API chaining."""
        config = (
            Config()
            .with_api_key("openai", "sk-test")
            .with_api_key("anthropic", "sk-anthropic")
            .with_alias("gpt-4-turbo", "gpt-4-0125-preview")
            .with_routing_rule("primary", 10, ["gpt-4"])
            .with_quota("default", rpm=60)
            .with_default_provider("openai")
        )

        assert config._api_keys == {"openai": "sk-test", "anthropic": "sk-anthropic"}
        assert config._model_aliases == {"gpt-4-turbo": "gpt-4-0125-preview"}
        assert len(config._routing_rules) == 1
        assert "default" in config._quotas
        assert config._default_provider == "openai"
