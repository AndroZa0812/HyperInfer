# hyperinfer-langchain

[![PyPI](https://img.shields.io/pypi/v/hyperinfer-langchain?style=flat-square)](https://pypi.org/project/hyperinfer-langchain/)
[![Python Versions](https://img.shields.io/pypi/pyversions/hyperinfer-langchain?style=flat-square)](https://pypi.org/project/hyperinfer-langchain/)

LangChain integration for HyperInfer LLM Gateway — wraps `HyperInferClient` as a drop-in LangChain `BaseChatModel`.

## Installation

```bash
pip install hyperinfer-langchain
```

## Usage

```python
import asyncio

from hyperinfer import Config
from hyperinfer_langchain import HyperInferChatModel
from langchain_core.messages import HumanMessage


async def main():
    config = Config().with_api_key("openai", "sk-...").with_alias("fast", "gpt-4o-mini")

    llm = await HyperInferChatModel.from_config(
        config=config,
        model="fast",
        virtual_key="my-team",
    )

    # Non-streaming
    response = llm.invoke([HumanMessage(content="Hello!")])
    print(response.content)

    # Streaming
    for chunk in llm.stream([HumanMessage(content="Tell me a joke")]):
        print(chunk.content, end="", flush=True)


asyncio.run(main())
```

## License

MIT
