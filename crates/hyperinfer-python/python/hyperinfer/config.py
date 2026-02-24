"""Configuration classes for HyperInfer."""

from typing import Any, Optional


class Config:
    """Configuration builder for HyperInfer client.

    Provides a fluent API for configuring API keys, routing rules,
    quotas, and model aliases.
    """

    def __init__(self):
        self._api_keys: dict[str, str] = {}
        self._routing_rules: list[dict[str, Any]] = []
        self._quotas: dict[str, dict[str, Optional[int]]] = {}
        self._model_aliases: dict[str, str] = {}
        self._default_provider: Optional[str] = None

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

    def with_routing_rule(
        self, name: str, priority: int, fallbacks: list[str]
    ) -> "Config":
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
        rpm: Optional[int] = None,
        tpm: Optional[int] = None,
        budget_cents: Optional[int] = None,
        max_requests_per_minute: Optional[int] = None,
        max_tokens_per_minute: Optional[int] = None,
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
            "max_requests_per_minute": max_requests_per_minute or rpm,
            "max_tokens_per_minute": max_tokens_per_minute or tpm,
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
