"""HyperInfer LlamaIndex integration."""

from dataclasses import dataclass
from typing import Any, Optional

from hyperinfer import Config


@dataclass
class LLMMetadata:
    """Metadata for LLM."""

    context_window: int = 4096
    num_tokens: Optional[int] = None
    is_chat_model: bool = True
    is_function_calling_model: bool = False


class HyperInferLLM:
    """LlamaIndex-compatible LLM using HyperInfer.

    This class provides integration with LlamaIndex's LLM interface.
    """

    def __init__(
        self,
        model: str,
        context_window: int = 4096,
        temperature: float = 0.7,
        max_tokens: Optional[int] = None,
        config: Optional[Config] = None,
    ):
        self.model = model
        self.temperature = temperature
        self.max_tokens = max_tokens
        self.config = config
        self.metadata = LLMMetadata(context_window=context_window)

    def complete(self, prompt: str, **kwargs: Any) -> str:
        """Complete the given prompt."""
        return f"Response to: {prompt}"

    def chat(self, messages: list[dict[str, str]], **kwargs: Any) -> str:
        """Chat with the given messages."""
        return f"Response to: {messages}"

    def __repr__(self) -> str:
        return f"HyperInferLLM(model={self.model!r}, context_window={self.metadata.context_window})"
