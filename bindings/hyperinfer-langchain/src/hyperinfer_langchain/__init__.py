"""LangChain integration for HyperInfer."""

from __future__ import annotations

from collections.abc import AsyncIterator, Iterator
from typing import Any, cast

from hyperinfer import Client, Config
from langchain_community.adapters.openai import convert_message_to_dict
from langchain_core.callbacks.manager import (
    AsyncCallbackManagerForLLMRun,
    CallbackManagerForLLMRun,
)
from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.messages import AIMessage, AIMessageChunk, BaseMessage
from langchain_core.outputs import ChatGeneration, ChatGenerationChunk, ChatResult
from pydantic import Field, model_validator

from ._sync import run_sync, run_sync_generator


def _format_messages(messages: list[BaseMessage]) -> list[dict[str, Any]]:
    """Convert a list of LangChain ``BaseMessage`` objects to OpenAI-style dicts.

    Delegates to :func:`langchain_community.adapters.openai.convert_message_to_dict`
    which handles all standard message types (Human, AI, System, Tool, Function,
    Chat) and preserves extra fields such as ``tool_calls``, ``tool_call_id``,
    and ``function_call`` from ``additional_kwargs``.
    """
    return [convert_message_to_dict(msg) for msg in messages]


def _infer_provider(model: str) -> str:
    """Infer provider from model name."""
    if "claude" in model.lower():
        return "anthropic"
    if "gemini" in model.lower():
        return "gemini"
    return "openai"


class HyperInferChatModel(BaseChatModel):
    """LangChain chat model backed by HyperInfer."""

    client: Client | None = Field(default=None)
    model: str = Field(default="gpt-4")
    virtual_key: str = Field(default="default")
    temperature: float | None = Field(default=None)
    max_tokens: int | None = Field(default=None)
    api_key: str | None = Field(default=None)
    api_keys: dict[str, str] | None = Field(default=None)
    default_provider: str | None = Field(default=None)
    redis_url: str | None = Field(default=None)
    config: Config | None = Field(default=None)

    @model_validator(mode="after")
    def _ensure_client(self) -> HyperInferChatModel:
        """Ensure a Client instance exists after construction."""
        if self.client is not None:
            return self

        # Build api_keys dict from individual api_key if provided
        keys = dict(self.api_keys) if self.api_keys else {}
        if self.api_key:
            provider = self.default_provider or _infer_provider(self.model)
            keys[provider] = self.api_key

        # Pass through None so Client honors REDIS_URL from the environment
        redis = self.redis_url

        # Build client
        if self.config is not None:
            # Merge api_keys onto config
            if keys:
                merged_config = Config(
                    api_keys={**self.config.to_dict().get("api_keys", {}), **keys},
                    model_aliases=self.config.to_dict().get("model_aliases"),
                    routing_rules=self.config.to_dict().get("routing_rules"),
                    quotas=self.config.to_dict().get("quotas"),
                    default_provider=self.default_provider
                    or self.config.to_dict().get("default_provider"),
                )
            else:
                merged_config = self.config
            self.client = Client(
                redis_url=redis, config=merged_config, virtual_key=self.virtual_key
            )
        elif keys or self.default_provider:
            self.client = Client(
                redis_url=redis,
                api_keys=keys,
                default_provider=self.default_provider,
                virtual_key=self.virtual_key,
            )
        else:
            self.client = Client(redis_url=redis, virtual_key=self.virtual_key)

        return self

    @property
    def _llm_type(self) -> str:
        return "hyperinfer"

    def _generate(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: CallbackManagerForLLMRun | None = None,
        **kwargs: Any,
    ) -> ChatResult:
        # run_manager here is a sync CallbackManagerForLLMRun; _agenerate
        # expects the async variant, so we pass None — the non-streaming path
        # does not invoke any token callbacks.
        return run_sync(self._agenerate(messages, stop, None, **kwargs))

    async def _agenerate(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: AsyncCallbackManagerForLLMRun | None = None,
        **kwargs: Any,
    ) -> ChatResult:
        formatted_messages = _format_messages(messages)

        try:
            if self.client is None:
                raise RuntimeError("Client not initialized")
            response = await self.client.chat(
                key=self.virtual_key,
                model=self.model,
                messages=formatted_messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
                stop=stop,
            )
        except Exception as e:
            raise RuntimeError(f"Chat request failed: {e}") from e

        try:
            content = response.get("choices", [{}])[0].get("message", {}).get("content", "")
        except (KeyError, IndexError, TypeError) as e:
            raise RuntimeError(f"Invalid response structure: {e}") from e

        ai_message = AIMessage(content=content)
        generation = ChatGeneration(message=ai_message)

        return ChatResult(generations=[generation])

    def _stream(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: CallbackManagerForLLMRun | None = None,
        **kwargs: Any,
    ) -> Iterator[ChatGenerationChunk]:
        """Synchronous streaming — yields chunks as they arrive from the async generator.

        Uses the shared persistent background event loop so chunks are
        forwarded incrementally rather than collected into a list first.  Safe
        to call from both plain-sync and already-running-async contexts
        (FastAPI, Jupyter, LangGraph nodes).
        """

        async def _producer() -> AsyncIterator[ChatGenerationChunk]:
            # run_manager here is a sync CallbackManagerForLLMRun; _astream
            # expects the async variant, so we pass None — token callbacks
            # are not available when bridging from a sync context.
            async for chunk in self._astream(messages, stop, None, **kwargs):
                yield chunk

        for chunk in run_sync_generator(_producer):
            yield cast(ChatGenerationChunk, chunk)

    async def _astream(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: AsyncCallbackManagerForLLMRun | None = None,
        **kwargs: Any,
    ) -> AsyncIterator[ChatGenerationChunk]:
        """Async token-by-token streaming via the HyperInfer data plane."""
        formatted_messages = _format_messages(messages)

        try:
            if self.client is None:
                raise RuntimeError("Client not initialized")
            async for chunk in self.client.stream(
                key=self.virtual_key,
                model=self.model,
                messages=formatted_messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
                stop=stop,
            ):
                delta = chunk.get("delta", "")
                finish_reason = chunk.get("finish_reason")
                ai_chunk = AIMessageChunk(content=delta)
                gen_chunk = ChatGenerationChunk(
                    message=ai_chunk,
                    generation_info=({"finish_reason": finish_reason} if finish_reason else None),
                )
                if run_manager:
                    await run_manager.on_llm_new_token(delta, chunk=gen_chunk)
                yield gen_chunk
        except Exception as e:
            raise RuntimeError(f"Streaming request failed: {e}") from e

    @classmethod
    async def from_config(
        cls,
        config: Config,
        model: str = "gpt-4",
        virtual_key: str = "default",
        redis_url: str = "redis://localhost:6379",
        **kwargs: Any,
    ) -> HyperInferChatModel:
        """Create instance with configuration."""
        client = Client(redis_url, config)
        await client.init()

        instance = cls(
            client=client,
            model=model,
            virtual_key=virtual_key,
            **kwargs,
        )

        return instance


__all__ = ["HyperInferChatModel"]
