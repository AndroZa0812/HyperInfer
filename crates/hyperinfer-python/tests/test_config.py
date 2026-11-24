"""Tests for Config class."""

import os
from unittest import mock

from hyperinfer.config import Config


class TestConfigInit:
    """Test Config initialization with kwargs."""

    def test_init_empty(self):
        """Test Config initialization with no args."""
        config = Config()
        assert config._api_keys == {}
        assert config._routing_rules == []
        assert config._quotas == {}
        assert config._model_aliases == {}
        assert config._default_provider is None

    def test_init_with_api_keys(self):
        """Test Config initialization with api_keys."""
        config = Config(api_keys={"openai": "sk-test", "anthropic": "sk-ant"})
        assert config._api_keys == {"openai": "sk-test", "anthropic": "sk-ant"}

    def test_init_with_model_aliases(self):
        """Test Config initialization with model_aliases."""
        config = Config(model_aliases={"fast": "gpt-4o-mini"})
        assert config._model_aliases == {"fast": "gpt-4o-mini"}

    def test_init_with_routing_rules(self):
        """Test Config initialization with routing_rules."""
        rules = [{"name": "primary", "priority": 10, "fallback_models": ["gpt-4"]}]
        config = Config(routing_rules=rules)
        assert config._routing_rules == rules

    def test_init_with_quotas_shorthand(self):
        """Test Config initialization with shorthand quota keys."""
        config = Config(quotas={"default": {"rpm": 60, "tpm": 100000}})
        quota = config._quotas["default"]
        assert quota["max_requests_per_minute"] == 60
        assert quota["max_tokens_per_minute"] == 100000

    def test_init_with_quotas_full_keys(self):
        """Test Config initialization with full quota key names."""
        config = Config(
            quotas={"default": {"max_requests_per_minute": 60, "max_tokens_per_minute": 100000}}
        )
        quota = config._quotas["default"]
        assert quota["max_requests_per_minute"] == 60
        assert quota["max_tokens_per_minute"] == 100000

    def test_init_with_default_provider(self):
        """Test Config initialization with default_provider."""
        config = Config(default_provider="openai")
        assert config._default_provider == "openai"

    def test_init_all_kwargs(self):
        """Test Config initialization with all kwargs."""
        config = Config(
            api_keys={"openai": "sk-test"},
            model_aliases={"fast": "gpt-4o-mini"},
            routing_rules=[{"name": "primary", "priority": 10, "fallback_models": ["gpt-4"]}],
            quotas={"default": {"rpm": 60}},
            default_provider="openai",
        )
        assert config._api_keys == {"openai": "sk-test"}
        assert config._model_aliases == {"fast": "gpt-4o-mini"}
        assert len(config._routing_rules) == 1
        assert config._quotas["default"]["max_requests_per_minute"] == 60
        assert config._default_provider == "openai"


class TestConfigFromEnv:
    """Test Config.from_env()."""

    def test_from_env_no_vars(self):
        """Test from_env with no environment variables set."""
        with mock.patch.dict(os.environ, {}, clear=True):
            config = Config.from_env()
            assert config._api_keys == {}
            assert config._default_provider is None

    def test_from_env_openai_key(self):
        """Test from_env reads OPENAI_API_KEY."""
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "sk-env-test"}, clear=True):
            config = Config.from_env()
            assert config._api_keys == {"openai": "sk-env-test"}

    def test_from_env_anthropic_key(self):
        """Test from_env reads ANTHROPIC_API_KEY."""
        with mock.patch.dict(os.environ, {"ANTHROPIC_API_KEY": "sk-ant-env"}, clear=True):
            config = Config.from_env()
            assert config._api_keys == {"anthropic": "sk-ant-env"}

    def test_from_env_gemini_key(self):
        """Test from_env reads GEMINI_API_KEY."""
        with mock.patch.dict(os.environ, {"GEMINI_API_KEY": "gemini-env"}, clear=True):
            config = Config.from_env()
            assert config._api_keys == {"gemini": "gemini-env"}

    def test_from_env_google_key(self):
        """Test from_env reads GOOGLE_API_KEY as gemini."""
        with mock.patch.dict(os.environ, {"GOOGLE_API_KEY": "google-env"}, clear=True):
            config = Config.from_env()
            assert config._api_keys == {"gemini": "google-env"}

    def test_from_env_multiple_keys(self):
        """Test from_env reads multiple API keys."""
        with mock.patch.dict(
            os.environ,
            {
                "OPENAI_API_KEY": "sk-openai",
                "ANTHROPIC_API_KEY": "sk-ant",
                "GEMINI_API_KEY": "gemini-key",
            },
            clear=True,
        ):
            config = Config.from_env()
            assert config._api_keys == {
                "openai": "sk-openai",
                "anthropic": "sk-ant",
                "gemini": "gemini-key",
            }

    def test_from_env_default_provider(self):
        """Test from_env reads HYPERINFER_DEFAULT_PROVIDER."""
        with mock.patch.dict(
            os.environ,
            {"OPENAI_API_KEY": "sk-test", "HYPERINFER_DEFAULT_PROVIDER": "openai"},
            clear=True,
        ):
            config = Config.from_env()
            assert config._default_provider == "openai"

    def test_from_env_default_provider_override(self):
        """Test from_env default_provider parameter overrides env var."""
        with mock.patch.dict(
            os.environ,
            {"OPENAI_API_KEY": "sk-test", "HYPERINFER_DEFAULT_PROVIDER": "openai"},
            clear=True,
        ):
            config = Config.from_env(default_provider="anthropic")
            assert config._default_provider == "anthropic"


class TestConfigFluentAPI:
    """Test backward compatibility of fluent API."""

    def test_with_api_key(self):
        """Test adding API key."""
        config = Config()
        result = config.with_api_key("openai", "sk-test-key")
        assert config._api_keys == {"openai": "sk-test-key"}
        assert result is config

    def test_with_alias(self):
        """Test adding model alias."""
        config = Config()
        result = config.with_alias("gpt-4-turbo", "gpt-4-0125-preview")
        assert config._model_aliases == {"gpt-4-turbo": "gpt-4-0125-preview"}
        assert result is config

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

    def test_with_quota_all_params(self):
        """Test adding quota with all parameters."""
        config = Config()
        result = config.with_quota("default", rpm=60, tpm=100000, budget_cents=1000)
        quota = config._quotas["default"]
        assert quota["max_requests_per_minute"] == 60
        assert quota["max_tokens_per_minute"] == 100000
        assert quota["budget_cents"] == 1000
        assert result is config

    def test_with_default_provider(self):
        """Test setting default provider."""
        config = Config()
        result = config.with_default_provider("openai")
        assert config._default_provider == "openai"
        assert result is config

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
