"""Tests for LlamaIndex integration."""

from unittest.mock import AsyncMock, patch

import pytest
from hyperinfer import Config
from hyperinfer_llamaindex import HyperInferLLM
from llama_index.core.llms import CompletionResponse


class TestRunSyncBridge:
    """Verify complete / stream_complete are safe when called from an async context."""

    @pytest.mark.asyncio
    async def test_complete_safe_inside_running_loop(self):
        """complete() must not raise 'This event loop is already running'."""
        llm = HyperInferLLM(model="gpt-4")

        with patch.object(llm, "_acomplete", new_callable=AsyncMock) as mock:
            mock.return_value = CompletionResponse(text="ok")
            # Calling the *sync* method from inside a running async test should
            # not raise RuntimeError even though there is already a loop.
            result = llm.complete("hello")
            assert result.text == "ok"
            mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_stream_complete_safe_inside_running_loop(self):
        """stream_complete() must not raise 'This event loop is already running'."""
        llm = HyperInferLLM(model="gpt-4")

        async def mock_stream(**kwargs):
            yield {
                "delta": "hello",
                "finish_reason": None,
                "usage": None,
                "id": "1",
                "model": "gpt-4",
            }
            yield {
                "delta": "!",
                "finish_reason": "stop",
                "usage": None,
                "id": "1",
                "model": "gpt-4",
            }

        with patch.object(llm.client, "stream", side_effect=mock_stream):
            chunks = list(llm.stream_complete("hi"))

        assert len(chunks) == 2
        assert chunks[-1].text == "hello!"


class TestHyperInferLLM:
    """Test suite for HyperInferLLM."""

    def test_metadata(self):
        """Test that metadata returns correct values."""
        llm = HyperInferLLM()
        metadata = llm.metadata
        assert metadata.context_window == 4096
        assert metadata.num_output == 256
        assert metadata.model_name == "gpt-4"

    def test_default_attributes(self):
        """Test default attribute values."""
        llm = HyperInferLLM()
        assert llm.model == "gpt-4"
        assert llm.virtual_key == "default"
        assert llm.temperature is None
        assert llm.max_tokens is None
        assert llm.context_window == 4096
        assert llm.num_output == 256

    def test_custom_attributes(self):
        """Test custom attribute values."""
        llm = HyperInferLLM(
            model="claude-3",
            virtual_key="test-key",
            temperature=0.7,
            max_tokens=100,
            context_window=8192,
            num_output=512,
        )
        assert llm.model == "claude-3"
        assert llm.virtual_key == "test-key"
        assert llm.temperature == 0.7
        assert llm.max_tokens == 100
        assert llm.context_window == 8192
        assert llm.num_output == 512

    @pytest.mark.asyncio
    async def test_acomplete(self):
        """Test async completion."""
        llm = HyperInferLLM(
            model="gpt-4",
            virtual_key="test-key",
        )

        mock_response = {"choices": [{"message": {"content": "Hello, world!"}}]}

        with patch.object(llm.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            result = await llm._acomplete("Hello")

            assert isinstance(result, CompletionResponse)
            assert result.text == "Hello, world!"
            mock_chat.assert_called_once()
            call_kwargs = mock_chat.call_args.kwargs
            assert call_kwargs["key"] == "test-key"
            assert call_kwargs["model"] == "gpt-4"
            assert call_kwargs["messages"] == [{"role": "user", "content": "Hello"}]

    @pytest.mark.asyncio
    async def test_acomplete_with_temperature(self):
        """Test async completion with temperature parameter."""
        llm = HyperInferLLM(
            model="gpt-4",
            temperature=0.5,
        )

        mock_response = {"choices": [{"message": {"content": "Response"}}]}

        with patch.object(llm.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            await llm._acomplete("Test")

            call_kwargs = mock_chat.call_args.kwargs
            assert call_kwargs["temperature"] == 0.5

    @pytest.mark.asyncio
    async def test_acomplete_with_max_tokens(self):
        """Test async completion with max_tokens parameter."""
        llm = HyperInferLLM(
            model="gpt-4",
            max_tokens=50,
        )

        mock_response = {"choices": [{"message": {"content": "Response"}}]}

        with patch.object(llm.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            await llm._acomplete("Test")

            call_kwargs = mock_chat.call_args.kwargs
            assert call_kwargs["max_tokens"] == 50

    @pytest.mark.asyncio
    async def test_acomplete_error_handling(self):
        """Test error handling in async completion."""
        llm = HyperInferLLM(model="gpt-4")

        with patch.object(llm.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.side_effect = Exception("API error")

            with pytest.raises(RuntimeError) as exc_info:
                await llm._acomplete("Test")

            assert "LLM completion failed" in str(exc_info.value)

    def test_complete_sync(self):
        """Test synchronous completion."""
        llm = HyperInferLLM(model="gpt-4")

        mock_response = {"choices": [{"message": {"content": "Sync response"}}]}

        with patch.object(llm, "_acomplete", new_callable=AsyncMock) as mock_acomplete:
            mock_acomplete.return_value = CompletionResponse(
                text="Sync response", raw=mock_response
            )

            result = llm.complete("Test")

            assert result.text == "Sync response"
            mock_acomplete.assert_called_once()

    def test_from_config(self):
        """Test creating instance from config."""
        from hyperinfer import Client, Config

        config = Config()
        with patch("hyperinfer_llamaindex.Client", spec=Client) as MockClient:
            llm = HyperInferLLM.from_config(
                config=config,
                model="claude-3",
                virtual_key="my-key",
            )

            assert llm.model == "claude-3"
            assert llm.virtual_key == "my-key"
            MockClient.assert_called_once_with(redis_url="redis://localhost:6379", config=config)

    def test_from_config_custom_redis_url(self):
        """Test creating instance from config with custom redis URL."""
        from hyperinfer import Client, Config

        config = Config()
        with patch("hyperinfer_llamaindex.Client", spec=Client) as MockClient:
            HyperInferLLM.from_config(
                config=config,
                redis_url="redis://custom:6379",
            )

            MockClient.assert_called_once_with(redis_url="redis://custom:6379", config=config)

    @pytest.mark.asyncio
    async def test_acomplete_empty_response(self):
        """Test handling of empty response."""
        llm = HyperInferLLM(model="gpt-4")

        mock_response = {"choices": []}

        with patch.object(llm.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            result = await llm._acomplete("Test")

            assert result.text == ""

    @pytest.mark.asyncio
    async def test_acomplete_response_without_message(self):
        """Test handling of response without message."""
        llm = HyperInferLLM(model="gpt-4")

        mock_response = {"choices": [{}]}

        with patch.object(llm.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            result = await llm._acomplete("Test")

            assert result.text == ""


class TestHyperInferLLMStreaming:
    """Tests for astream_complete and stream_complete."""

    @pytest.mark.asyncio
    async def test_astream_complete_yields_cumulative_text(self):
        """Each chunk should carry cumulative text and the incremental delta."""
        llm = HyperInferLLM(model="gpt-4", virtual_key="test-key")

        chunks = [
            {
                "id": "1",
                "model": "gpt-4",
                "delta": "The ",
                "finish_reason": None,
                "usage": None,
            },
            {
                "id": "1",
                "model": "gpt-4",
                "delta": "answer",
                "finish_reason": None,
                "usage": None,
            },
            {
                "id": "1",
                "model": "gpt-4",
                "delta": " is 42",
                "finish_reason": "stop",
                "usage": None,
            },
        ]

        async def mock_stream(**kwargs):
            for chunk in chunks:
                yield chunk

        with patch.object(llm.client, "stream", side_effect=mock_stream):
            gen = await llm.astream_complete("What is the answer?")
            results = [r async for r in gen]

        assert len(results) == 3
        assert all(isinstance(r, CompletionResponse) for r in results)
        assert results[0].delta == "The "
        assert results[1].delta == "answer"
        assert results[2].delta == " is 42"
        assert results[0].text == "The "
        assert results[1].text == "The answer"
        assert results[2].text == "The answer is 42"

    @pytest.mark.asyncio
    async def test_astream_complete_empty_delta_on_final_chunk(self):
        """Empty delta on the final usage chunk still yields a response."""
        llm = HyperInferLLM(model="gpt-4")

        chunks = [
            {
                "id": "1",
                "model": "gpt-4",
                "delta": "Hi",
                "finish_reason": None,
                "usage": None,
            },
            {
                "id": "1",
                "model": "gpt-4",
                "delta": "",
                "finish_reason": "stop",
                "usage": {"input_tokens": 3, "output_tokens": 1},
            },
        ]

        async def mock_stream(**kwargs):
            for chunk in chunks:
                yield chunk

        with patch.object(llm.client, "stream", side_effect=mock_stream):
            gen = await llm.astream_complete("Hello")
            results = [r async for r in gen]

        assert len(results) == 2
        assert results[-1].text == "Hi"

    @pytest.mark.asyncio
    async def test_astream_complete_propagates_errors(self):
        """Provider errors are wrapped as RuntimeError."""
        llm = HyperInferLLM(model="gpt-4")

        async def mock_stream(**kwargs):
            raise ConnectionError("network failure")
            yield  # make it a generator

        with patch.object(llm.client, "stream", side_effect=mock_stream):
            with pytest.raises(RuntimeError, match="Streaming completion failed"):
                gen = await llm.astream_complete("Hi")
                async for _ in gen:
                    pass


class TestPythonicConstructor:
    """Test Pythonic direct constructor without from_config."""

    def test_constructor_with_api_key(self):
        """Test constructor with api_key parameter."""
        llm = HyperInferLLM(model="gpt-4", api_key="sk-test")
        assert llm.client is not None
        assert llm.model == "gpt-4"

    def test_constructor_with_api_keys(self):
        """Test constructor with api_keys dict."""
        llm = HyperInferLLM(
            model="gpt-4",
            api_keys={"openai": "sk-test", "anthropic": "sk-ant"},
        )
        assert llm.client is not None

    def test_constructor_with_default_provider(self):
        """Test constructor with default_provider."""
        llm = HyperInferLLM(
            model="gpt-4",
            api_key="sk-test",
            default_provider="openai",
        )
        assert llm.client is not None

    def test_constructor_with_redis_url(self):
        """Test constructor with custom redis_url."""
        llm = HyperInferLLM(
            model="gpt-4",
            api_key="sk-test",
            redis_url="redis://custom:6379",
        )
        assert llm.client is not None
        assert llm.client._redis_url == "redis://custom:6379"

    def test_constructor_with_config(self):
        """Test constructor with Config object."""
        config = Config().with_api_key("openai", "sk-test")
        llm = HyperInferLLM(model="gpt-4", config=config)
        assert llm.client is not None

    def test_constructor_with_explicit_client(self):
        """Test constructor with explicit client (no auto-creation)."""
        from hyperinfer import Client

        explicit_client = Client(redis_url="redis://explicit:6379")
        llm = HyperInferLLM(model="gpt-4", client=explicit_client)
        assert llm.client is explicit_client

    def test_constructor_infers_anthropic_provider(self):
        """Test provider inference for Claude models."""
        llm = HyperInferLLM(model="claude-3-5-sonnet-20241022", api_key="sk-ant")
        assert llm.client is not None

    def test_constructor_infers_gemini_provider(self):
        """Test provider inference for Gemini models."""
        llm = HyperInferLLM(model="gemini-1.5-pro", api_key="gemini-key")
        assert llm.client is not None

    def test_constructor_infers_openai_provider(self):
        """Test provider inference for OpenAI models (default)."""
        llm = HyperInferLLM(model="gpt-4", api_key="sk-test")
        assert llm.client is not None

    def test_constructor_honors_redis_url_env(self, monkeypatch):
        """Test constructor honors REDIS_URL env var when redis_url is omitted."""
        monkeypatch.setenv("REDIS_URL", "redis://env-host:6380")
        llm = HyperInferLLM(model="gpt-4", api_key="sk-test")
        assert llm.client is not None
        assert llm.client._redis_url == "redis://env-host:6380"

    def test_secrets_hidden_from_repr_and_dump(self):
        """Test api_key and api_keys are excluded from repr() and model_dump()."""
        llm = HyperInferLLM(
            model="gpt-4",
            api_key="sk-super-secret",
            api_keys={"anthropic": "sk-ant-secret"},
        )
        assert "sk-super-secret" not in repr(llm)
        assert "sk-ant-secret" not in repr(llm)
        dumped = llm.model_dump()
        assert "api_key" not in dumped
        assert "api_keys" not in dumped
