# hyperinfer-langchain

LangChain integration for HyperInfer LLM Gateway.

## Installation

```bash
pip install hyperinfer-langchain
```

## Usage

```python
import asyncio

from hyperinfer import Config
from hyperinfer_langchain import HyperInferChatModel

async def main():
    # Build a config with your API keys and routing rules.
    config = (
        Config()
        .with_api_key("openai", "sk-...")
        .with_alias("fast", "gpt-4o-mini")
    )

    # Create the chat model (async factory — respects existing event loops).
    llm = await HyperInferChatModel.from_config(
        config=config,
        model="fast",
        virtual_key="my-team",
    )

    # Use like any LangChain chat model.
    from langchain_core.messages import HumanMessage

    response = llm.invoke([HumanMessage(content="Hello!")])
    print(response.content)

    # Streaming
    for chunk in llm.stream([HumanMessage(content="Tell me a joke")]):
        print(chunk.content, end="", flush=True)

asyncio.run(main())
```
