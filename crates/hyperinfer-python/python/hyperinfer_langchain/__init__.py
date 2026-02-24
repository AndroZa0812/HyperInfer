"""HyperInfer LangChain integration."""

from typing import Any, Optional

from hyperinfer import Config


class HyperInferChatModel:
    """LangChain-compatible chat model using HyperInfer.

    This class provides integration with LangChain's chat model interface.
    """

    def __init__(
        self,
        model: str,
        temperature: float = 0.7,
        max_tokens: Optional[int] = None,
        config: Optional[Config] = None,
    ):
        self.model = model
        self.temperature = temperature
        self.max_tokens = max_tokens
        self.config = config
        self._llm_type = "hyperinfer"

    def _generate(self, prompts: list[str], **kwargs: Any) -> list[str]:
        """Generate responses for the given prompts."""
        return [f"Response to: {prompt}" for prompt in prompts]

    def __repr__(self) -> str:
        return (
            f"HyperInferChatModel(model={self.model!r}, temperature={self.temperature})"
        )
