"""Tests for LangChain integration."""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from langchain_core.messages import AIMessage, HumanMessage, SystemMessage

from hyperinfer_langchain import HyperInferChatModel


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

        mock_response = {
            "choices": [{"message": {"content": "Response with system context"}}]
        }

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

        mock_response = {
            "choices": [{"message": {"content": "Continuing conversation"}}]
        }

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

        mock_response = {"choices": [{"message": {"content": "Sync response"}}]}

        with patch.object(
            model, "_agenerate", new_callable=AsyncMock
        ) as mock_agenerate:
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

    def test_from_config(self):
        """Test creating instance from config."""
        from hyperinfer import Client, Config

        with patch("hyperinfer_langchain.Client") as MockClient:
            mock_client_instance = MagicMock(spec=Client)
            mock_client_instance.init = AsyncMock()
            MockClient.return_value = mock_client_instance

            with patch("asyncio.run") as mock_run:
                config = Config()
                model = HyperInferChatModel.from_config(
                    config=config,
                    model="claude-3",
                    virtual_key="my-key",
                )

                assert model.model == "claude-3"
                assert model.virtual_key == "my-key"
                MockClient.assert_called_once_with("redis://localhost:6379")
                mock_run.assert_called_once()
