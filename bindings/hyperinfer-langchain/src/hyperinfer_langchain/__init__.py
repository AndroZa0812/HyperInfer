"""LangChain integration for HyperInfer."""

from __future__ import annotations

import asyncio
import concurrent.futures
from collections.abc import AsyncIterator, Iterator
from typing import Any, cast

from hyperinfer import Client, Config
from langchain_core.language_models.chat_models import BaseChatModel
from langchain_core.messages import (
    AIMessage,
    AIMessageChunk,
    BaseMessage,
    HumanMessage,
    SystemMessage,
)
from langchain_core.outputs import ChatGeneration, ChatGenerationChunk, ChatResult
from pydantic import Field


def _run_sync(coro: Any) -> Any:
    """Run *coro* safely from any context — sync or already-async.

    ``asyncio.run()`` raises ``RuntimeError: This event loop is already
    running`` when called from inside an async context (FastAPI, Jupyter,
    LangGraph, etc.).  This helper avoids that by delegating to a
    *dedicated background thread* that owns its own event loop, then blocks
    the current thread until the result is ready.

    Because we use a fresh thread, there is never a loop conflict regardless
    of what the caller's thread is doing.
    """
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
        return pool.submit(asyncio.run, coro).result()


class HyperInferChatModel(BaseChatModel):
    """LangChain chat model backed by HyperInfer."""

    client: Client = Field(default_factory=Client)
    model: str = Field(default="gpt-4")
    virtual_key: str = Field(default="default")
    temperature: float | None = Field(default=None)
    max_tokens: int | None = Field(default=None)

    @property
    def _llm_type(self) -> str:
        return "hyperinfer"

    def _generate(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: Any = None,
        **kwargs: Any,
    ) -> ChatResult:
        return cast(
            ChatResult,
            _run_sync(self._agenerate(messages, stop, run_manager, **kwargs)),
        )

    async def _agenerate(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: Any = None,
        **kwargs: Any,
    ) -> ChatResult:
        formatted_messages: list[dict[str, str]] = []
        for msg in messages:
            if isinstance(msg, HumanMessage):
                formatted_messages.append({"role": "user", "content": str(msg.content)})
            elif isinstance(msg, AIMessage):
                formatted_messages.append(
                    {"role": "assistant", "content": str(msg.content)}
                )
            elif isinstance(msg, SystemMessage):
                formatted_messages.append(
                    {"role": "system", "content": str(msg.content)}
                )
            else:
                formatted_messages.append({"role": "user", "content": str(msg.content)})

        try:
            response = await self.client.chat(
                key=self.virtual_key,
                model=self.model,
                messages=formatted_messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
            )
        except Exception as e:
            raise RuntimeError(f"Chat request failed: {e}") from e

        try:
            content = (
                response.get("choices", [{}])[0].get("message", {}).get("content", "")
            )
        except (KeyError, IndexError, TypeError) as e:
            raise RuntimeError(f"Invalid response structure: {e}") from e

        ai_message = AIMessage(content=content)
        generation = ChatGeneration(message=ai_message)

        return ChatResult(generations=[generation])

    def _stream(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: Any = None,
        **kwargs: Any,
    ) -> Iterator[ChatGenerationChunk]:
        """Synchronous streaming — drives the async generator on a background thread.

        Safe to call from both plain-sync and already-running-async contexts
        (FastAPI, Jupyter, LangGraph nodes).  Prefer :meth:`_astream` when
        already inside an async context to avoid the thread-pool overhead.
        """

        async def _collect() -> list[ChatGenerationChunk]:
            return [
                chunk
                async for chunk in self._astream(messages, stop, run_manager, **kwargs)
            ]

        yield from _run_sync(_collect())

    async def _astream(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: Any = None,
        **kwargs: Any,
    ) -> AsyncIterator[ChatGenerationChunk]:
        """Async token-by-token streaming via the HyperInfer data plane."""
        formatted_messages: list[dict[str, str]] = []
        for msg in messages:
            if isinstance(msg, HumanMessage):
                formatted_messages.append({"role": "user", "content": str(msg.content)})
            elif isinstance(msg, AIMessage):
                formatted_messages.append(
                    {"role": "assistant", "content": str(msg.content)}
                )
            elif isinstance(msg, SystemMessage):
                formatted_messages.append(
                    {"role": "system", "content": str(msg.content)}
                )
            else:
                formatted_messages.append({"role": "user", "content": str(msg.content)})

        try:
            async for chunk in self.client.stream(
                key=self.virtual_key,
                model=self.model,
                messages=formatted_messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
            ):
                delta = chunk.get("delta", "")
                finish_reason = chunk.get("finish_reason")
                ai_chunk = AIMessageChunk(content=delta)
                gen_chunk = ChatGenerationChunk(
                    message=ai_chunk,
                    generation_info=(
                        {"finish_reason": finish_reason} if finish_reason else None
                    ),
                )
                if run_manager:
                    run_manager.on_llm_new_token(delta, chunk=gen_chunk)
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
        client = Client(redis_url)
        await client.init()

        instance = cls(
            client=client,
            model=model,
            virtual_key=virtual_key,
            **kwargs,
        )

        return instance


__all__ = ["HyperInferChatModel"]
