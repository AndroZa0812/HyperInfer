"""Pytest configuration for LlamaIndex tests.

Mocks the ``hyperinfer`` package so the test suite runs without requiring
the compiled Rust extension (``.so`` / ``.pyd``) to be installed.
"""

import sys
from unittest.mock import AsyncMock, MagicMock

mock_inner_client = MagicMock()
mock_inner_client.chat = AsyncMock(
    return_value={
        "choices": [{"message": {"content": "test response"}, "finish_reason": "stop"}],
        "usage": {"input_tokens": 10, "output_tokens": 20},
    }
)


class MockClient:
    """Drop-in replacement for ``hyperinfer.Client`` in tests."""

    def __init__(self, *args, **kwargs):
        self._instance = mock_inner_client

    async def init(self) -> None:
        pass

    async def chat(self, key: str, model: str, messages: list, **kwargs):
        return await self._instance.chat(
            key, {"model": model, "messages": messages, **kwargs}
        )

    async def __aenter__(self):
        await self.init()
        return self

    async def __aexit__(self, *args):
        pass

    def __getattr__(self, name):
        return getattr(self._instance, name)


class MockConfig:
    """Drop-in replacement for ``hyperinfer.Config`` in tests."""


# Register mocks before any test module imports hyperinfer.
sys.modules["hyperinfer"] = MagicMock()
sys.modules["hyperinfer"].Client = MockClient
sys.modules["hyperinfer"].Config = MockConfig
