"""High-level async client for HyperInfer."""

import asyncio
import os
from collections.abc import AsyncIterator, Mapping, Sequence
from typing import TYPE_CHECKING, Any

from hyperinfer.config import Config, Quota, RoutingRule

# This block is seen by IDEs/Linters but ignored at runtime
if TYPE_CHECKING:
    from hyperinfer._hyperinfer import HyperInferClient


def __getattr__(name: str) -> type["HyperInferClient"]:
    """Lazy-load HyperInferClient from the native Rust extension."""
    if name == "HyperInferClient":
        from hyperinfer._hyperinfer import HyperInferClient as _HyperInferClient

        return _HyperInferClient
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


class Client:
    """Async client wrapper for HyperInfer.

    Provides a simplified async interface for interacting with the LLM gateway.
    """

    def __init__(
        self,
        redis_url: str | None = None,
        config: Config | None = None,
        *,
        api_keys: Mapping[str, str] | None = None,
        model_aliases: Mapping[str, str] | None = None,
        routing_rules: Sequence[RoutingRule | dict[str, Any]] | None = None,
        quotas: Mapping[str, Quota | dict[str, int | None]] | None = None,
        default_provider: str | None = None,
        virtual_key: str = "default",
    ) -> None:
        """Initialize the client.

        Args:
            redis_url: Redis connection URL. Defaults to REDIS_URL env var or
                "redis://localhost:6379".
            config: A Config object. If None and any of the keyword overrides
                are provided, a Config is constructed from them.
            api_keys: Provider API keys (e.g. {"openai": "sk-..."}).
            model_aliases: Model alias mappings.
            routing_rules: Routing rule definitions.
            quotas: Quota configurations.
            default_provider: Default provider name.
            virtual_key: Virtual key for authentication/quota tracking.
        """
        self._redis_url: str = redis_url or os.getenv("REDIS_URL") or "redis://localhost:6379"
        self._virtual_key: str = virtual_key
        self._config_dict: dict[str, Any] | None = None

        # Determine config dict
        if config is not None:
            # Start with provided config
            config_dict = config.to_dict()
            # Merge keyword overrides
            if api_keys:
                config_dict["api_keys"] = {**config_dict.get("api_keys", {}), **api_keys}
            if model_aliases:
                config_dict["model_aliases"] = {
                    **config_dict.get("model_aliases", {}),
                    **model_aliases,
                }
            if routing_rules:
                normalized_rules = [Config._normalize_routing_rule(r) for r in routing_rules]
                config_dict["routing_rules"] = (
                    config_dict.get("routing_rules", []) + normalized_rules
                )
            if quotas:
                normalized_quotas = {k: Config._normalize_quota(v) for k, v in quotas.items()}
                config_dict["quotas"] = {**config_dict.get("quotas", {}), **normalized_quotas}
            if default_provider:
                config_dict["default_provider"] = default_provider
            self._config_dict = config_dict
        elif api_keys or model_aliases or routing_rules or quotas or default_provider:
            # No config object, but keyword overrides provided — build Config
            self._config_dict = Config(
                api_keys=api_keys,
                model_aliases=model_aliases,
                routing_rules=routing_rules,
                quotas=quotas,
                default_provider=default_provider,
            ).to_dict()
        else:
            # No config and no overrides — try from_env
            env_config = Config.from_env()
            env_dict = env_config.to_dict()
            # Only set if env vars were actually found
            if env_dict["api_keys"] or env_dict["default_provider"]:
                self._config_dict = env_dict
            else:
                self._config_dict = None

        self._inner: Any = None
        self._initialized = False
        self._lifecycle_lock = asyncio.Lock()
        self._init_lock = asyncio.Lock()

    async def init(self) -> None:
        """Initialize the client connection."""
        async with self._init_lock:
            if self._initialized:
                return
            if self._inner is None:
                from hyperinfer._hyperinfer import HyperInferClient

                self._inner = HyperInferClient(
                    self._redis_url or "redis://localhost:6379",
                    self._config_dict if self._config_dict is not None else {},
                )
            await self._inner.init()
            self._initialized = True

    async def chat(
        self,
        key: str | None = None,
        model: str | list[dict[str, Any]] | None = None,
        messages: list[dict[str, Any]] | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        stop: list[str] | None = None,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: Any = None,
        response_format: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Send a chat request to the LLM gateway.

        Supports multiple calling conventions:
        - Legacy: chat(key, model, messages)
        - Pythonic: chat(model="gpt-4", messages=[...])
        - Shorthand: chat("gpt-4", messages)

        Args:
            key: API key for authentication. Defaults to self._virtual_key.
            model: Model identifier (e.g., "gpt-4", "claude-3").
            messages: List of message dicts with "role" and "content" keys.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.
            stop: Stop sequences; generation halts when any is produced.
            tools: Tool definitions for function calling.
            tool_choice: Tool selection strategy.
            response_format: Response format specification.

        Returns:
            Response dictionary containing model output and usage info.
        """
        # Handle 2-arg positional shorthand: chat("gpt-4", messages)
        if isinstance(model, list) and messages is None:
            messages = model
            model = key
            key = None

        if key is None:
            key = self._virtual_key

        async with self._lifecycle_lock:
            if not self._initialized:
                await self.init()
            inner = self._inner

        request: dict[str, Any] = {
            "model": model,
            "messages": messages,
        }
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens
        if stop is not None:
            request["stop"] = stop
        if tools is not None:
            request["tools"] = tools
        if tool_choice is not None:
            request["tool_choice"] = tool_choice
        if response_format is not None:
            request["response_format"] = response_format

        return await inner.chat(key, request)  # type: ignore[no-any-return]

    async def stream(
        self,
        key: str | None = None,
        model: str | list[dict[str, Any]] | None = None,
        messages: list[dict[str, Any]] | None = None,
        temperature: float | None = None,
        max_tokens: int | None = None,
        stop: list[str] | None = None,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: Any = None,
        response_format: dict[str, Any] | None = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """Stream token chunks from the LLM gateway.

        Yields one dict per SSE event with the following keys:

        - ``id`` (str): Stream identifier (same across all chunks).
        - ``model`` (str): Model that produced the chunk.
        - ``delta`` (str): Incremental text content for this chunk.
        - ``finish_reason`` (str | None): ``"stop"`` on the last chunk.
        - ``usage`` (dict | None): Token counts on the final chunk only.

        Supports multiple calling conventions:
        - Legacy: stream(key, model, messages)
        - Pythonic: stream(model="gpt-4", messages=[...])
        - Shorthand: stream("gpt-4", messages)

        Args:
            key: Virtual key for authentication / quota tracking.
            model: Model identifier (e.g., ``"gpt-4"``).
            messages: Conversation history as role/content dicts.
            temperature: Sampling temperature (0.0–2.0).
            max_tokens: Maximum tokens to generate.
            stop: Stop sequences; generation halts when any is produced.
            tools: Tool definitions for function calling.
            tool_choice: Tool selection strategy.
            response_format: Response format specification.

        Example::

            async for chunk in client.stream("my-key", "gpt-4", messages):
                print(chunk["delta"], end="", flush=True)
        """
        # Handle 2-arg positional shorthand: stream("gpt-4", messages)
        if isinstance(model, list) and messages is None:
            messages = model
            model = key
            key = None

        if key is None:
            key = self._virtual_key

        async with self._lifecycle_lock:
            if not self._initialized:
                await self.init()
            inner = self._inner

        request: dict[str, Any] = {"model": model, "messages": messages}
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens
        if stop is not None:
            request["stop"] = stop
        if tools is not None:
            request["tools"] = tools
        if tool_choice is not None:
            request["tool_choice"] = tool_choice
        if response_format is not None:
            request["response_format"] = response_format

        chunk_iter = await inner.chat_stream(key, request)
        async for chunk in chunk_iter:
            yield chunk

    async def set_mirror(self, model: str | None = None, sample_rate: float | None = None) -> None:
        """Configure traffic mirroring for the client.

        Args:
            model: Target model for the shadow request.
            sample_rate: Fraction of requests to mirror in [0.0, 1.0].
        """
        async with self._lifecycle_lock:
            if not self._initialized:
                await self.init()

            await self._inner.set_mirror(model, sample_rate)

    async def __aenter__(self) -> "Client":
        """Async context manager entry."""
        await self.init()
        return self

    async def close(self) -> None:
        """Close the client connection and cleanup resources."""
        async with self._lifecycle_lock:
            if hasattr(self._inner, "close"):
                await self._inner.close()
            self._inner = None
            self._initialized = False

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Async context manager exit."""
        await self.close()
