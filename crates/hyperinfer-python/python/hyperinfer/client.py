"""High-level async client for HyperInfer."""

import asyncio
from typing import Any, Optional

from hyperinfer._hyperinfer import HyperInferClient


class Client:
    """Async client wrapper for HyperInfer.

    Provides a simplified async interface for interacting with the LLM gateway.
    """

    def __init__(self, redis_url: str = "redis://localhost:6379"):
        """Initialize the client.

        Args:
            redis_url: Redis connection URL for the backend.
        """
        self._inner = HyperInferClient(redis_url)
        self._initialized = False

    async def init(self) -> None:
        """Initialize the client connection."""
        if self._initialized:
            return
        await self._inner.init()
        self._initialized = True

    async def chat(
        self,
        key: str,
        model: str,
        messages: list[dict[str, str]],
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
    ) -> dict[str, Any]:
        """Send a chat request to the LLM gateway.

        Args:
            key: API key for authentication.
            model: Model identifier (e.g., "gpt-4", "claude-3").
            messages: List of message dicts with "role" and "content" keys.
            temperature: Sampling temperature (0.0-2.0).
            max_tokens: Maximum tokens to generate.

        Returns:
            Response dictionary containing model output and usage info.
        """
        if not self._initialized:
            await self.init()

        request = {
            "model": model,
            "messages": messages,
        }
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens

        return await self._inner.chat(key, request)

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
