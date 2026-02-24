"""LlamaIndex integration for HyperInfer."""

from __future__ import annotations
from typing import Any
from llama_index.core.llms import CustomLLM, CompletionResponse, LLMMetadata
from llama_index.core.llms.callbacks import llm_completion_callback
from pydantic import Field

from hyperinfer import Client, Config


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
    def complete(
        self, prompt: str, formatted: bool = False, **kwargs: Any
    ) -> CompletionResponse:
        import asyncio

        return asyncio.run(self._acomplete(prompt, **kwargs))

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
    ) -> CompletionResponse:
        import asyncio

        return asyncio.run(self._astream_complete(prompt, **kwargs))

    @llm_completion_callback()
    async def _astream_complete(self, prompt: str, **kwargs: Any) -> CompletionResponse:
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

    @classmethod
    def from_config(
        cls,
        config: Config,
        model: str = "gpt-4",
        virtual_key: str = "default",
        redis_url: str = "redis://localhost:6379",
        **kwargs: Any,
    ) -> "HyperInferLLM":
        client = Client(redis_url)
        instance = cls(client=client, model=model, virtual_key=virtual_key, **kwargs)
        import asyncio

        asyncio.run(client.init(config))
        return instance


__all__ = ["HyperInferLLM"]
