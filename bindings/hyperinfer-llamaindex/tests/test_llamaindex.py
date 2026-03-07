"""Tests for LlamaIndex integration."""

from unittest.mock import AsyncMock, patch

import pytest
from hyperinfer_llamaindex import HyperInferLLM
from llama_index.core.llms import CompletionResponse


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
            {"id": "1", "model": "gpt-4", "delta": "The ", "finish_reason": None, "usage": None},
            {"id": "1", "model": "gpt-4", "delta": "answer", "finish_reason": None, "usage": None},
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
            {"id": "1", "model": "gpt-4", "delta": "Hi", "finish_reason": None, "usage": None},
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
