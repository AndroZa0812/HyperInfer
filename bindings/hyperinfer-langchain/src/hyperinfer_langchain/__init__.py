"""LangChain integration for HyperInfer."""

from __future__ import annotations

import asyncio
import concurrent.futures
import queue
import threading
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
from pydantic import Field


def _format_messages(messages: list[BaseMessage]) -> list[dict[str, Any]]:
    """Convert a list of LangChain ``BaseMessage`` objects to OpenAI-style dicts.

    Delegates to :func:`langchain_community.adapters.openai.convert_message_to_dict`
    which handles all standard message types (Human, AI, System, Tool, Function,
    Chat) and preserves extra fields such as ``tool_calls``, ``tool_call_id``,
    and ``function_call`` from ``additional_kwargs``.
    """
    return [convert_message_to_dict(msg) for msg in messages]


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
        run_manager: CallbackManagerForLLMRun | None = None,
        **kwargs: Any,
    ) -> ChatResult:
        # run_manager here is a sync CallbackManagerForLLMRun; _agenerate
        # expects the async variant, so we pass None — the non-streaming path
        # does not invoke any token callbacks.
        return cast(
            ChatResult,
            _run_sync(self._agenerate(messages, stop, None, **kwargs)),
        )

    async def _agenerate(
        self,
        messages: list[BaseMessage],
        stop: list[str] | None = None,
        run_manager: AsyncCallbackManagerForLLMRun | None = None,
        **kwargs: Any,
    ) -> ChatResult:
        formatted_messages = _format_messages(messages)

        try:
            response = await self.client.chat(
                key=self.virtual_key,
                model=self.model,
                messages=formatted_messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
                **({"stop": stop} if stop else {}),
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

        Uses a background thread + bounded ``queue.Queue`` so chunks are
        forwarded incrementally rather than collected into a list first.  Safe
        to call from both plain-sync and already-running-async contexts
        (FastAPI, Jupyter, LangGraph nodes).

        Backpressure and cancellation
        ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
        The queue is bounded (size 1) so the producer blocks after each chunk
        until the consumer takes it.  A ``cancel_event`` is set by the
        consumer whenever it exits early (break, exception, or
        ``GeneratorExit``), allowing the producer to stop iterating before the
        upstream stream finishes instead of running to completion and buffering
        all remaining chunks.
        """
        _sentinel = object()
        # Bounded to 1: producer blocks until the consumer takes each chunk,
        # providing natural backpressure and bounding peak memory to one chunk.
        chunk_queue: queue.Queue[ChatGenerationChunk | BaseException | object] = queue.Queue(
            maxsize=1
        )
        cancel_event = threading.Event()

        async def _producer() -> None:
            try:
                # run_manager here is a sync CallbackManagerForLLMRun; _astream
                # expects the async variant, so we pass None — token callbacks
                # are not available when bridging from a sync context.
                async for chunk in self._astream(messages, stop, None, **kwargs):
                    if cancel_event.is_set():
                        break
                    chunk_queue.put(chunk)
            except Exception as exc:  # noqa: BLE001
                chunk_queue.put(exc)
            finally:
                chunk_queue.put(_sentinel)

        def _run_producer() -> None:
            asyncio.run(_producer())

        t = threading.Thread(target=_run_producer, daemon=True)
        t.start()

        try:
            while True:
                item = chunk_queue.get()
                if item is _sentinel:
                    break
                if isinstance(item, BaseException):
                    raise item
                yield cast(ChatGenerationChunk, item)
        except GeneratorExit:
            cancel_event.set()
        except BaseException:
            cancel_event.set()
            raise
        finally:
            # Drain the queue so the producer thread is never blocked on a
            # put() and can observe cancel_event / reach the sentinel.
            while not chunk_queue.empty():
                try:
                    chunk_queue.get_nowait()
                except queue.Empty:
                    break
            t.join()

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
            async for chunk in self.client.stream(
                key=self.virtual_key,
                model=self.model,
                messages=formatted_messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
                **({"stop": stop} if stop else {}),
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
