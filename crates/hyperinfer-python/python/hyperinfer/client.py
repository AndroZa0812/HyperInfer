"""High-level async client for HyperInfer."""

import asyncio
from collections.abc import AsyncIterator
from typing import Any

from hyperinfer._hyperinfer import HyperInferClient
from hyperinfer.config import Config


class Client:
    """Async client wrapper for HyperInfer.

    Provides a simplified async interface for interacting with the LLM gateway.
    """

    def __init__(
        self,
        redis_url: str = "redis://localhost:6379",
        config: Config | None = None,
    ):
        """Initialize the client.

        Args:
            redis_url: Redis connection URL for the backend.
            config: Optional :class:`Config` instance.  API keys, routing rules,
                model aliases, and quotas are read from this object and passed
                directly to the Rust data plane on initialisation.
        """
        config_dict = config.to_dict() if config is not None else None
        self._inner = HyperInferClient(redis_url, config_dict)
        self._initialized = False
        self._init_lock = asyncio.Lock()

    async def init(self) -> None:
        """Initialize the client connection.

        Uses double-checked locking so concurrent coroutines do not race to
        initialise the underlying Rust client more than once.
        """
        if self._initialized:
            return
        async with self._init_lock:
            if self._initialized:
                return
            await self._inner.init()
            self._initialized = True

    async def chat(
        self,
        key: str,
        model: str,
        messages: list[dict[str, str]],
        temperature: float | None = None,
        max_tokens: int | None = None,
        stop: list[str] | None = None,
    ) -> dict[str, Any]:
        """Send a chat request to the LLM gateway.

        Args:
            key: API key for authentication.
            model: Model identifier (e.g., "gpt-4", "claude-3").
            messages: List of message dicts with "role" and "content" keys.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.
            stop: Stop sequences; generation halts when any is produced.

        Returns:
            Response dictionary containing model output and usage info.
        """
        if not self._initialized:
            await self.init()

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

        return await self._inner.chat(key, request)

    async def stream(
        self,
        key: str,
        model: str,
        messages: list[dict[str, str]],
        temperature: float | None = None,
        max_tokens: int | None = None,
        stop: list[str] | None = None,
    ) -> AsyncIterator[dict[str, Any]]:
        """Stream token chunks from the LLM gateway.

        Yields one dict per SSE event with the following keys:

        - ``id`` (str): Stream identifier (same across all chunks).
        - ``model`` (str): Model that produced the chunk.
        - ``delta`` (str): Incremental text content for this chunk.
        - ``finish_reason`` (str | None): ``"stop"`` on the last chunk.
        - ``usage`` (dict | None): Token counts on the final chunk only.

        Args:
            key: Virtual key for authentication / quota tracking.
            model: Model identifier (e.g., ``"gpt-4"``).
            messages: Conversation history as role/content dicts.
            temperature: Sampling temperature (0.0–2.0).
            max_tokens: Maximum tokens to generate.
            stop: Stop sequences; generation halts when any is produced.

        Example::

            async for chunk in client.stream("my-key", "gpt-4", messages):
                print(chunk["delta"], end="", flush=True)
        """
        if not self._initialized:
            await self.init()

        request: dict[str, Any] = {"model": model, "messages": messages}
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens
        if stop is not None:
            request["stop"] = stop

        chunk_iter = await self._inner.chat_stream(key, request)
        async for chunk in chunk_iter:
            yield chunk

    async def __aenter__(self) -> "Client":
        """Async context manager entry."""
        await self.init()
        return self

    async def close(self) -> None:
        """Close the client connection and cleanup resources."""
        if hasattr(self._inner, "close"):
            await self._inner.close()
        self._initialized = False

    async def __aexit__(self, exc_type: Any, exc_val: Any, exc_tb: Any) -> None:
        """Async context manager exit."""
        await self.close()
