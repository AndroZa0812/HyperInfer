"""High-level async client for HyperInfer."""

import asyncio
from collections.abc import AsyncIterator
from typing import TYPE_CHECKING, Any

from hyperinfer.config import Config

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
        redis_url: str = "redis://localhost:6379",
        config: Config | None = None,
    ):
        """Initialize the client."""
        self._config_dict = config.to_dict() if config is not None else None
        self._redis_url = redis_url
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

                self._inner = HyperInferClient(self._redis_url, self._config_dict)
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
        async with self._lifecycle_lock:
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

            return await self._inner.chat(key, request)  # type: ignore[no-any-return]

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
        async with self._lifecycle_lock:
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
