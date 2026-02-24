"""HyperInfer LangChain integration."""

from typing import Any, Optional

from hyperinfer import Client, Config
from langchain_core.messages import AIMessage
from langchain_core.outputs import ChatGeneration, ChatResult


class HyperInferChatModel:
    """LangChain-compatible chat model using HyperInfer.

    This class provides integration with LangChain's chat model interface.
    """

    def __init__(
        self,
        model: str = "gpt-4",
        temperature: Optional[float] = None,
        max_tokens: Optional[int] = None,
        config: Optional[Config] = None,
        virtual_key: str = "default",
        client: Optional[Client] = None,
    ):
        self.model = model
        self.temperature = temperature
        self.max_tokens = max_tokens
        self.config = config
        self.virtual_key = virtual_key
        self.client = client if client is not None else Client()
        self._llm_type = "hyperinfer"

    @classmethod
    def from_config(
        cls,
        config: Config,
        model: str = "gpt-4",
        virtual_key: str = "default",
        **kwargs: Any,
    ) -> "HyperInferChatModel":
        """Create instance from config."""
        import asyncio

        client = Client("redis://localhost:6379")
        asyncio.run(client.init())

        return cls(
            client=client,
            model=model,
            virtual_key=virtual_key,
            **kwargs,
        )

    def _generate(self, messages: list, **kwargs: Any):
        import asyncio

        return asyncio.run(self._agenerate(messages, **kwargs))

    async def _agenerate(self, messages: list, **kwargs: Any):
        formatted_messages = []
        for msg in messages:
            if hasattr(msg, "type"):
                if msg.type == "human":
                    formatted_messages.append({"role": "user", "content": msg.content})
                elif msg.type == "ai":
                    formatted_messages.append(
                        {"role": "assistant", "content": msg.content}
                    )
                elif msg.type == "system":
                    formatted_messages.append(
                        {"role": "system", "content": msg.content}
                    )
                else:
                    formatted_messages.append(
                        {"role": "user", "content": str(msg.content)}
                    )
            else:
                formatted_messages.append({"role": "user", "content": str(msg)})

        response = await self.client.chat(
            key=self.virtual_key,
            model=self.model,
            messages=formatted_messages,
            temperature=self.temperature,
            max_tokens=self.max_tokens,
        )

        content = response.get("choices", [{}])[0].get("message", {}).get("content", "")
        ai_message = AIMessage(content=content)
        generation = ChatGeneration(message=ai_message)

        return ChatResult(generations=[generation])

    def __repr__(self) -> str:
        return (
            f"HyperInferChatModel(model={self.model!r}, temperature={self.temperature})"
        )
