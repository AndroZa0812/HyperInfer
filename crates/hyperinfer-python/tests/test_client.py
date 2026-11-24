"""Tests for Client class."""

import os
from unittest import mock

from hyperinfer import Client
from hyperinfer.config import Config


class TestClientInit:
    """Test Client initialization."""

    def test_init_defaults(self):
        """Test Client initialization with defaults."""
        with mock.patch.dict(os.environ, {}, clear=True):
            client = Client()
        assert client._redis_url == "redis://localhost:6379"
        assert client._virtual_key == "default"
        assert client._config_dict is None

    def test_init_with_redis_url(self):
        """Test Client initialization with custom redis URL."""
        client = Client(redis_url="redis://custom:6379")
        assert client._redis_url == "redis://custom:6379"

    def test_init_with_virtual_key(self):
        """Test Client initialization with custom virtual key."""
        client = Client(virtual_key="my-key")
        assert client._virtual_key == "my-key"

    def test_init_with_config(self):
        """Test Client initialization with Config object."""
        config = Config().with_api_key("openai", "sk-test")
        client = Client(config=config)
        assert client._config_dict["api_keys"] == {"openai": "sk-test"}

    def test_init_with_api_keys_kwarg(self):
        """Test Client initialization with api_keys kwarg."""
        client = Client(api_keys={"openai": "sk-test"})
        assert client._config_dict["api_keys"] == {"openai": "sk-test"}

    def test_init_with_model_aliases_kwarg(self):
        """Test Client initialization with model_aliases kwarg."""
        client = Client(model_aliases={"fast": "gpt-4o-mini"})
        assert client._config_dict["model_aliases"] == {"fast": "gpt-4o-mini"}

    def test_init_with_routing_rules_kwarg(self):
        """Test Client initialization with routing_rules kwarg."""
        rules = [{"name": "primary", "priority": 10, "fallback_models": ["gpt-4"]}]
        client = Client(routing_rules=rules)
        assert client._config_dict["routing_rules"] == rules

    def test_init_with_quotas_kwarg(self):
        """Test Client initialization with quotas kwarg."""
        client = Client(quotas={"default": {"rpm": 60}})
        assert client._config_dict["quotas"]["default"]["max_requests_per_minute"] == 60

    def test_init_with_default_provider_kwarg(self):
        """Test Client initialization with default_provider kwarg."""
        client = Client(default_provider="openai")
        assert client._config_dict["default_provider"] == "openai"

    def test_init_config_with_overrides(self):
        """Test Client initialization with config and keyword overrides."""
        config = Config().with_api_key("openai", "sk-original")
        client = Client(config=config, api_keys={"anthropic": "sk-ant"})
        assert client._config_dict["api_keys"]["openai"] == "sk-original"
        assert client._config_dict["api_keys"]["anthropic"] == "sk-ant"

    def test_init_from_env(self):
        """Test Client initialization reads from environment."""
        with mock.patch.dict(
            os.environ,
            {"OPENAI_API_KEY": "sk-env", "HYPERINFER_DEFAULT_PROVIDER": "openai"},
            clear=True,
        ):
            client = Client()
        assert client._config_dict["api_keys"] == {"openai": "sk-env"}
        assert client._config_dict["default_provider"] == "openai"

    def test_init_from_env_no_vars(self):
        """Test Client initialization with no env vars results in None config."""
        with mock.patch.dict(os.environ, {}, clear=True):
            client = Client()
        assert client._config_dict is None


class TestClientChat:
    """Test Client.chat() method."""

    def test_chat_legacy_style(self):
        """Test chat with legacy 3-arg style."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat = mock.AsyncMock(return_value={"choices": []})
        client._initialized = True

        import asyncio

        asyncio.run(
            client.chat(
                key="my-key",
                model="gpt-4",
                messages=[{"role": "user", "content": "Hello"}],
            )
        )

        client._inner.chat.assert_called_once_with(
            "my-key",
            {"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]},
        )

    def test_chat_pythonic_style(self):
        """Test chat with Pythonic keyword style."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat = mock.AsyncMock(return_value={"choices": []})
        client._initialized = True

        import asyncio

        asyncio.run(
            client.chat(
                model="gpt-4",
                messages=[{"role": "user", "content": "Hello"}],
            )
        )

        client._inner.chat.assert_called_once_with(
            "default",
            {"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]},
        )

    def test_chat_shorthand_style(self):
        """Test chat with shorthand 2-arg style."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat = mock.AsyncMock(return_value={"choices": []})
        client._initialized = True

        import asyncio

        asyncio.run(client.chat("gpt-4", [{"role": "user", "content": "Hello"}]))

        client._inner.chat.assert_called_once_with(
            "default",
            {"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]},
        )

    def test_chat_with_tools(self):
        """Test chat with tools parameter."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat = mock.AsyncMock(return_value={"choices": []})
        client._initialized = True

        import asyncio

        tools = [{"type": "function", "function": {"name": "test"}}]
        asyncio.run(
            client.chat(
                model="gpt-4",
                messages=[{"role": "user", "content": "Hello"}],
                tools=tools,
            )
        )

        call_args = client._inner.chat.call_args
        assert call_args[0][1]["tools"] == tools

    def test_chat_with_tool_choice(self):
        """Test chat with tool_choice parameter."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat = mock.AsyncMock(return_value={"choices": []})
        client._initialized = True

        import asyncio

        asyncio.run(
            client.chat(
                model="gpt-4",
                messages=[{"role": "user", "content": "Hello"}],
                tool_choice="auto",
            )
        )

        call_args = client._inner.chat.call_args
        assert call_args[0][1]["tool_choice"] == "auto"

    def test_chat_with_response_format(self):
        """Test chat with response_format parameter."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat = mock.AsyncMock(return_value={"choices": []})
        client._initialized = True

        import asyncio

        response_format = {"type": "json_object"}
        asyncio.run(
            client.chat(
                model="gpt-4",
                messages=[{"role": "user", "content": "Hello"}],
                response_format=response_format,
            )
        )

        call_args = client._inner.chat.call_args
        assert call_args[0][1]["response_format"] == response_format


class TestClientStream:
    """Test Client.stream() method."""

    @staticmethod
    async def _empty_aiter():
        if False:
            yield {}

    def test_stream_pythonic_style(self):
        """Test stream with Pythonic keyword style."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat_stream = mock.AsyncMock(return_value=self._empty_aiter())
        client._initialized = True

        import asyncio

        async def collect():
            return [
                chunk
                async for chunk in client.stream(
                    model="gpt-4",
                    messages=[{"role": "user", "content": "Hello"}],
                )
            ]

        asyncio.run(collect())

        client._inner.chat_stream.assert_called_once_with(
            "default",
            {"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]},
        )

    def test_stream_shorthand_style(self):
        """Test stream with shorthand 2-arg style."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat_stream = mock.AsyncMock(return_value=self._empty_aiter())
        client._initialized = True

        import asyncio

        async def collect():
            return [
                chunk
                async for chunk in client.stream(
                    "gpt-4",
                    [{"role": "user", "content": "Hello"}],
                )
            ]

        asyncio.run(collect())

        client._inner.chat_stream.assert_called_once_with(
            "default",
            {"model": "gpt-4", "messages": [{"role": "user", "content": "Hello"}]},
        )

    def test_stream_with_tools(self):
        """Test stream with tools parameter."""
        client = Client()
        client._inner = mock.MagicMock()
        client._inner.chat_stream = mock.AsyncMock(return_value=self._empty_aiter())
        client._initialized = True

        import asyncio

        tools = [{"type": "function", "function": {"name": "test"}}]

        async def collect():
            return [
                chunk
                async for chunk in client.stream(
                    model="gpt-4",
                    messages=[{"role": "user", "content": "Hello"}],
                    tools=tools,
                )
            ]

        asyncio.run(collect())

        call_args = client._inner.chat_stream.call_args
        assert call_args[0][1]["tools"] == tools
