"""LlamaIndex integration for HyperInfer."""

from __future__ import annotations

import asyncio
import concurrent.futures
from typing import Any, cast

from hyperinfer import Client, Config


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


from llama_index.core.base.llms.types import (
    CompletionResponseAsyncGen,
    CompletionResponseGen,
)
from llama_index.core.llms import CompletionResponse, CustomLLM, LLMMetadata
from llama_index.core.llms.callbacks import llm_completion_callback
from pydantic import Field


class HyperInferLLM(CustomLLM):
    """LlamaIndex LLM backed by HyperInfer."""

    client: Client = Field(default_factory=Client)
    model: str = Field(default="gpt-4")
    virtual_key: str = Field(default="default")
    temperature: float | None = Field(default=None)
    max_tokens: int | None = Field(default=None)
    context_window: int = Field(default=4096)
    num_output: int = Field(default=256)

    @property
    def metadata(self) -> LLMMetadata:
        return LLMMetadata(
            context_window=self.context_window,
            num_output=self.num_output,
            model_name=self.model,
        )

    @llm_completion_callback()
    def complete(self, prompt: str, formatted: bool = False, **kwargs: Any) -> CompletionResponse:
        return cast(CompletionResponse, _run_sync(self._acomplete(prompt, **kwargs)))

    @llm_completion_callback()
    async def _acomplete(self, prompt: str, **kwargs: Any) -> CompletionResponse:
        try:
            messages = [{"role": "user", "content": prompt}]

            response = await self.client.chat(
                key=self.virtual_key,
                model=self.model,
                messages=messages,
                temperature=self.temperature,
                max_tokens=self.max_tokens,
            )

            choices = response.get("choices", [])
            if not choices:
                content = ""
            else:
                content = choices[0].get("message", {}).get("content", "")
            return CompletionResponse(text=content, raw=response)
        except Exception as e:
            raise RuntimeError(f"LLM completion failed: {e}") from e

    @llm_completion_callback()
    def stream_complete(
        self, prompt: str, formatted: bool = False, **kwargs: Any
    ) -> CompletionResponseGen:
        """Synchronous streaming — collects chunks from the async generator.

        Safe to call from both plain-sync and already-running-async contexts
        (FastAPI, Jupyter, LangGraph nodes).  Prefer :meth:`astream_complete`
        when already inside an async context to avoid the thread-pool overhead.
        """

        async def _collect() -> list[CompletionResponse]:
            return [
                r async for r in await self.astream_complete(prompt, formatted=formatted, **kwargs)
            ]

        def _gen() -> CompletionResponseGen:
            yield from _run_sync(_collect())

        return _gen()

    @llm_completion_callback()
    async def astream_complete(
        self, prompt: str, formatted: bool = False, **kwargs: Any
    ) -> CompletionResponseAsyncGen:
        """Async token-by-token streaming via the HyperInfer data plane.

        Returns an async generator where each :class:`CompletionResponse` has:

        - ``text``: cumulative text assembled so far.
        - ``delta``: the incremental token(s) for this chunk.
        - ``raw``: the raw chunk dict from the provider.
        """
        messages = [{"role": "user", "content": prompt}]
        client = self.client
        virtual_key = self.virtual_key
        model = self.model
        temperature = self.temperature
        max_tokens = self.max_tokens

        async def _gen() -> CompletionResponseAsyncGen:
            accumulated = ""
            try:
                async for chunk in client.stream(
                    key=virtual_key,
                    model=model,
                    messages=messages,
                    temperature=temperature,
                    max_tokens=max_tokens,
                ):
                    delta = chunk.get("delta", "")
                    accumulated += delta
                    yield CompletionResponse(text=accumulated, delta=delta, raw=chunk)
            except Exception as e:
                raise RuntimeError(f"Streaming completion failed: {e}") from e

        return _gen()

    @classmethod
    def from_config(
        cls,
        config: Config,
        model: str = "gpt-4",
        virtual_key: str = "default",
        redis_url: str = "redis://localhost:6379",
        **kwargs: Any,
    ) -> HyperInferLLM:
        """Create an instance with configuration.

        The underlying client is initialised lazily on the first call to
        :meth:`complete` or :meth:`_acomplete`, so this factory is safe to call
        from both sync and async contexts without risk of event-loop conflicts.
        """
        client = Client(redis_url=redis_url, config=config)
        return cls(client=client, model=model, virtual_key=virtual_key, **kwargs)


__all__ = ["HyperInferLLM"]
