"""Configuration classes for HyperInfer."""

import os
from typing import Any


class Config:
    """Configuration builder for HyperInfer client.

    Provides a fluent API for configuring API keys, routing rules,
    quotas, and model aliases.
    """

    def __init__(
        self,
        api_keys: dict[str, str] | None = None,
        model_aliases: dict[str, str] | None = None,
        routing_rules: list[dict[str, Any]] | None = None,
        quotas: dict[str, dict[str, int | None]] | None = None,
        default_provider: str | None = None,
    ) -> None:
        self._api_keys: dict[str, str] = dict(api_keys) if api_keys else {}
        self._routing_rules: list[dict[str, Any]] = list(routing_rules) if routing_rules else []
        self._quotas: dict[str, dict[str, int | None]] = {}
        self._model_aliases: dict[str, str] = dict(model_aliases) if model_aliases else {}
        self._default_provider: str | None = default_provider

        if quotas:
            for key, quota in quotas.items():
                self._quotas[key] = self._normalize_quota(quota)

    @staticmethod
    def _normalize_quota(quota: dict[str, int | None]) -> dict[str, int | None]:
        """Normalize quota dict, mapping shorthand keys to full names."""
        result: dict[str, int | None] = {}
        for k, v in quota.items():
            if k == "rpm":
                result["max_requests_per_minute"] = v
            elif k == "tpm":
                result["max_tokens_per_minute"] = v
            else:
                result[k] = v
        return result

    @classmethod
    def from_env(cls, *, default_provider: str | None = None) -> "Config":
        """Build a Config from environment variables.

        Reads the following environment variables:
        - OPENAI_API_KEY -> provider "openai"
        - ANTHROPIC_API_KEY -> provider "anthropic"
        - GEMINI_API_KEY or GOOGLE_API_KEY -> provider "gemini"
        - HYPERINFER_DEFAULT_PROVIDER -> default provider (if default_provider not provided)

        Args:
            default_provider: Override the default provider. If None, reads
                HYPERINFER_DEFAULT_PROVIDER from environment.

        Returns:
            A Config instance populated with API keys found in the environment.
        """
        api_keys: dict[str, str] = {}

        openai_key = os.getenv("OPENAI_API_KEY")
        if openai_key:
            api_keys["openai"] = openai_key

        anthropic_key = os.getenv("ANTHROPIC_API_KEY")
        if anthropic_key:
            api_keys["anthropic"] = anthropic_key

        gemini_key = os.getenv("GEMINI_API_KEY") or os.getenv("GOOGLE_API_KEY")
        if gemini_key:
            api_keys["gemini"] = gemini_key

        if default_provider is None:
            default_provider = os.getenv("HYPERINFER_DEFAULT_PROVIDER")

        return cls(api_keys=api_keys, default_provider=default_provider)

    def with_api_key(self, provider: str, key: str) -> "Config":
        """Add an API key for a provider.

        Args:
            provider: Provider name (e.g., "openai", "anthropic").
            key: API key for the provider.

        Returns:
            Self for method chaining.
        """
        self._api_keys[provider] = key
        return self

    def with_alias(self, alias: str, target: str) -> "Config":
        """Add a model alias mapping.

        Args:
            alias: Alias name (e.g., "gpt-4-turbo").
            target: Target model name (e.g., "gpt-4-0125-preview").

        Returns:
            Self for method chaining.
        """
        self._model_aliases[alias] = target
        return self

    def with_routing_rule(self, name: str, priority: int, fallbacks: list[str]) -> "Config":
        """Add a routing rule.

        Args:
            name: Rule name.
            priority: Priority (higher = more preferred).
            fallbacks: List of fallback model names.

        Returns:
            Self for method chaining.
        """
        self._routing_rules.append(
            {
                "name": name,
                "priority": priority,
                "fallback_models": fallbacks,
            }
        )
        return self

    def with_quota(
        self,
        key: str,
        rpm: int | None = None,
        tpm: int | None = None,
        budget_cents: int | None = None,
        max_requests_per_minute: int | None = None,
        max_tokens_per_minute: int | None = None,
    ) -> "Config":
        """Add a quota configuration.

        Args:
            key: Key identifier for the quota.
            rpm: Requests per minute limit.
            tpm: Tokens per minute limit.
            budget_cents: Monthly budget in cents (USD).
            max_requests_per_minute: Requests per minute limit (alias for rpm).
            max_tokens_per_minute: Tokens per minute limit (alias for tpm).

        Returns:
            Self for method chaining.
        """
        self._quotas[key] = {
            "max_requests_per_minute": (
                max_requests_per_minute if max_requests_per_minute is not None else rpm
            ),
            "max_tokens_per_minute": (
                max_tokens_per_minute if max_tokens_per_minute is not None else tpm
            ),
            "budget_cents": budget_cents,
        }
        return self

    def with_default_provider(self, provider: str) -> "Config":
        """Set the default provider.

        Args:
            provider: Provider name (e.g., "openai", "anthropic").

        Returns:
            Self for method chaining.
        """
        self._default_provider = provider
        return self

    def to_dict(self) -> dict[str, Any]:
        """Convert configuration to dictionary.

        Returns:
            Dictionary representation of the configuration.
        """
        return {
            "api_keys": self._api_keys,
            "routing_rules": self._routing_rules,
            "quotas": self._quotas,
            "model_aliases": self._model_aliases,
            "default_provider": self._default_provider,
        }
