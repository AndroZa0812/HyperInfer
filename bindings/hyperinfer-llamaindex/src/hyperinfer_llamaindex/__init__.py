"""LlamaIndex integration for HyperInfer."""

from __future__ import annotations

from typing import Any, cast

from hyperinfer import Client, Config
from llama_index.core.base.llms.types import (
    CompletionResponseAsyncGen,
    CompletionResponseGen,
)
from llama_index.core.llms import CompletionResponse, CustomLLM, LLMMetadata
from llama_index.core.llms.callbacks import llm_completion_callback
from pydantic import Field, model_validator

from ._sync import run_sync, run_sync_generator


def _infer_provider(model: str) -> str:
    """Infer provider from model name."""
    if "claude" in model.lower():
        return "anthropic"
    if "gemini" in model.lower():
        return "gemini"
    return "openai"


class HyperInferLLM(CustomLLM):
    """LlamaIndex LLM backed by HyperInfer."""

    client: Client | None = Field(default=None)
    model: str = Field(default="gpt-4")
    virtual_key: str = Field(default="default")
    temperature: float | None = Field(default=None)
    max_tokens: int | None = Field(default=None)
    context_window: int = Field(default=4096)
    num_output: int = Field(default=256)
    api_key: str | None = Field(default=None)
    api_keys: dict[str, str] | None = Field(default=None)
    default_provider: str | None = Field(default=None)
    redis_url: str | None = Field(default=None)
    config: Config | None = Field(default=None)

    @model_validator(mode="after")
    def _ensure_client(self) -> HyperInferLLM:
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
    def metadata(self) -> LLMMetadata:
        return LLMMetadata(
            context_window=self.context_window,
            num_output=self.num_output,
            model_name=self.model,
        )

    @llm_completion_callback()
    def complete(self, prompt: str, formatted: bool = False, **kwargs: Any) -> CompletionResponse:
        return cast(CompletionResponse, run_sync(self._acomplete(prompt, **kwargs)))

    @llm_completion_callback()
    async def _acomplete(self, prompt: str, **kwargs: Any) -> CompletionResponse:
        try:
            if self.client is None:
                raise RuntimeError("Client not initialized")
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
        """Synchronous streaming — yields chunks as they arrive from the async generator.

        Uses the shared persistent background event loop so chunks are
        forwarded incrementally rather than collected into a list first.  Safe
        to call from both plain-sync and already-running-async contexts
        (FastAPI, Jupyter, LangGraph nodes).
        """

        async def _producer() -> CompletionResponseAsyncGen:
            async for chunk in await self.astream_complete(prompt, formatted=formatted, **kwargs):
                yield chunk

        def _gen() -> CompletionResponseGen:
            for chunk in run_sync_generator(_producer):
                yield cast(CompletionResponse, chunk)

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
        if client is None:
            raise RuntimeError("HyperInfer client is not initialized")
        virtual_key = self.virtual_key
        model = self.model
        temperature = self.temperature
        max_tokens = self.max_tokens

        async def _gen() -> CompletionResponseAsyncGen:
            accumulated = ""
            if client is None:
                raise RuntimeError("Client not initialized")
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

        The underlying :class:`~hyperinfer.Client` is constructed eagerly here.
        Call :meth:`~hyperinfer.Client.init` (or use an ``async with`` block)
        before invoking :meth:`complete` or :meth:`_acomplete` if you need the
        connection to be established before the first request.
        """
        client = Client(redis_url=redis_url, config=config)
        return cls(client=client, model=model, virtual_key=virtual_key, **kwargs)


__all__ = ["HyperInferLLM"]
