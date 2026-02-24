"""Pytest configuration for LangChain tests."""

import sys
from unittest.mock import MagicMock, AsyncMock

mock_client = MagicMock()
mock_client.chat = AsyncMock(
    return_value={"choices": [{"message": {"content": "test"}}]}
)


class MockClient:
    def __init__(self, *args, **kwargs):
        self._instance = mock_client

    async def init(self):
        pass

    def __getattr__(self, name):
        return getattr(self._instance, name)


class MockConfig:
    pass


sys.modules["hyperinfer"] = MagicMock()
sys.modules["hyperinfer"].Client = MockClient
sys.modules["hyperinfer"].Config = MockConfig
