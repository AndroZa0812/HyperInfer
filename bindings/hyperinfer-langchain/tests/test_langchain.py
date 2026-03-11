"""Tests for LangChain integration."""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from hyperinfer_langchain import HyperInferChatModel
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage
from langchain_core.outputs import ChatGenerationChunk


class TestRunSyncBridge:
    """Verify _generate / _stream are safe when called from an async context."""

    @pytest.mark.asyncio
    async def test_generate_safe_inside_running_loop(self):
        """_generate must not raise 'This event loop is already running'."""
        model = HyperInferChatModel(model="gpt-4")

        with patch.object(model, "_agenerate", new_callable=AsyncMock) as mock:
            mock.return_value = MagicMock()
            # Calling the *sync* method from inside a running async test should
            # not raise RuntimeError even though there is already a loop.
            model._generate([HumanMessage(content="hi")])
            mock.assert_called_once()

    @pytest.mark.asyncio
    async def test_stream_safe_inside_running_loop(self):
        """_stream must not raise 'This event loop is already running'."""
        model = HyperInferChatModel(model="gpt-4")

        async def _fake_astream(*_args, **_kwargs):
            yield MagicMock(spec=ChatGenerationChunk, message=MagicMock(content="x"))

        with patch.object(model, "_astream", side_effect=_fake_astream):
            chunks = list(model._stream([HumanMessage(content="hi")]))
        assert len(chunks) == 1


class TestHyperInferChatModel:
    """Test suite for HyperInferChatModel."""

    def test_llm_type(self):
        """Test that _llm_type returns correct value."""
        model = HyperInferChatModel()
        assert model._llm_type == "hyperinfer"

    def test_default_attributes(self):
        """Test default attribute values."""
        model = HyperInferChatModel()
        assert model.model == "gpt-4"
        assert model.virtual_key == "default"
        assert model.temperature is None
        assert model.max_tokens is None

    def test_custom_attributes(self):
        """Test custom attribute values."""
        model = HyperInferChatModel(
            model="claude-3",
            virtual_key="test-key",
            temperature=0.7,
            max_tokens=100,
        )
        assert model.model == "claude-3"
        assert model.virtual_key == "test-key"
        assert model.temperature == 0.7
        assert model.max_tokens == 100

    @pytest.mark.asyncio
    async def test_agenerate_with_human_message(self):
        """Test async generation with HumanMessage."""
        model = HyperInferChatModel(
            model="gpt-4",
            virtual_key="test-key",
        )

        mock_response = {"choices": [{"message": {"content": "Hello, human!"}}]}

        with patch.object(model.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            messages = [HumanMessage(content="Hi")]
            result = await model._agenerate(messages)

            assert len(result.generations) == 1
            assert result.generations[0].message.content == "Hello, human!"
            mock_chat.assert_called_once()
            call_kwargs = mock_chat.call_args.kwargs
            assert call_kwargs["key"] == "test-key"
            assert call_kwargs["model"] == "gpt-4"

    @pytest.mark.asyncio
    async def test_agenerate_with_system_message(self):
        """Test async generation with SystemMessage."""
        model = HyperInferChatModel(model="gpt-4")

        mock_response = {"choices": [{"message": {"content": "Response with system context"}}]}

        with patch.object(model.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            messages = [
                SystemMessage(content="You are a helpful assistant"),
                HumanMessage(content="Hello"),
            ]
            result = await model._agenerate(messages)

            assert len(result.generations) == 1
            call_kwargs = mock_chat.call_args.kwargs
            assert len(call_kwargs["messages"]) == 2

    @pytest.mark.asyncio
    async def test_agenerate_with_ai_message(self):
        """Test async generation with AIMessage."""
        model = HyperInferChatModel(model="gpt-4")

        mock_response = {"choices": [{"message": {"content": "Continuing conversation"}}]}

        with patch.object(model.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            messages = [
                HumanMessage(content="Hello"),
                AIMessage(content="Hi there!"),
                HumanMessage(content="How are you?"),
            ]
            result = await model._agenerate(messages)

            assert len(result.generations) == 1
            call_kwargs = mock_chat.call_args.kwargs
            assert len(call_kwargs["messages"]) == 3

    def test_generate_sync(self):
        """Test synchronous generation."""
        model = HyperInferChatModel(model="gpt-4")

        with patch.object(model, "_agenerate", new_callable=AsyncMock) as mock_agenerate:
            mock_agenerate.return_value = MagicMock()

            messages = [HumanMessage(content="Test")]
            model._generate(messages)

            mock_agenerate.assert_called_once()

    @pytest.mark.asyncio
    async def test_agenerate_with_temperature(self):
        """Test async generation with temperature parameter."""
        model = HyperInferChatModel(
            model="gpt-4",
            temperature=0.5,
        )

        mock_response = {"choices": [{"message": {"content": "Response"}}]}

        with patch.object(model.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            messages = [HumanMessage(content="Test")]
            await model._agenerate(messages)

            call_kwargs = mock_chat.call_args.kwargs
            assert call_kwargs["temperature"] == 0.5

    @pytest.mark.asyncio
    async def test_agenerate_with_max_tokens(self):
        """Test async generation with max_tokens parameter."""
        model = HyperInferChatModel(
            model="gpt-4",
            max_tokens=50,
        )

        mock_response = {"choices": [{"message": {"content": "Response"}}]}

        with patch.object(model.client, "chat", new_callable=AsyncMock) as mock_chat:
            mock_chat.return_value = mock_response

            messages = [HumanMessage(content="Test")]
            await model._agenerate(messages)

            call_kwargs = mock_chat.call_args.kwargs
            assert call_kwargs["max_tokens"] == 50

    @pytest.mark.asyncio
    async def test_from_config(self):
        """Test creating instance from config."""
        from hyperinfer import Client, Config

        with patch("hyperinfer_langchain.Client") as MockClient:
            mock_client_instance = MagicMock(spec=Client)
            mock_client_instance.init = AsyncMock()
            MockClient.return_value = mock_client_instance

            config = Config()
            model = await HyperInferChatModel.from_config(
                config=config,
                model="claude-3",
                virtual_key="my-key",
            )

            assert model.model == "claude-3"
            assert model.virtual_key == "my-key"
            MockClient.assert_called_once_with("redis://localhost:6379", config)
            mock_client_instance.init.assert_called_once()


class TestHyperInferChatModelStreaming:
    """Tests for _astream and _stream."""

    @pytest.mark.asyncio
    async def test_astream_yields_chunks(self):
        """_astream yields ChatGenerationChunk for each delta."""
        model = HyperInferChatModel(model="gpt-4", virtual_key="test-key")

        chunks = [
            {
                "id": "1",
                "model": "gpt-4",
                "delta": "Hello",
                "finish_reason": None,
                "usage": None,
            },
            {
                "id": "1",
                "model": "gpt-4",
                "delta": " world",
                "finish_reason": None,
                "usage": None,
            },
            {
                "id": "1",
                "model": "gpt-4",
                "delta": "",
                "finish_reason": "stop",
                "usage": {"input_tokens": 5, "output_tokens": 2},
            },
        ]

        async def mock_stream(**kwargs):
            for chunk in chunks:
                yield chunk

        with patch.object(model.client, "stream", side_effect=mock_stream):
            messages = [HumanMessage(content="Hi")]
            result = []
            async for chunk in model._astream(messages):
                result.append(chunk)

        assert len(result) == 3
        assert all(isinstance(c, ChatGenerationChunk) for c in result)
        assert result[0].message.content == "Hello"
        assert result[1].message.content == " world"
        assert result[2].generation_info == {"finish_reason": "stop"}

    @pytest.mark.asyncio
    async def test_astream_concatenates_to_full_response(self):
        """Concatenating all deltas should produce the full response."""
        model = HyperInferChatModel(model="gpt-4")

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

        with patch.object(model.client, "stream", side_effect=mock_stream):
            result = []
            async for chunk in model._astream([HumanMessage(content="What?")]):
                result.append(chunk.message.content)

        assert "".join(result) == "The answer is 42"

    @pytest.mark.asyncio
    async def test_astream_propagates_errors(self):
        """_astream wraps provider errors as RuntimeError."""
        model = HyperInferChatModel(model="gpt-4")

        async def mock_stream(**kwargs):
            raise ValueError("provider down")
            yield  # make it a generator

        with patch.object(model.client, "stream", side_effect=mock_stream):
            with pytest.raises(RuntimeError, match="Streaming request failed"):
                async for _ in model._astream([HumanMessage(content="Hi")]):
                    pass
