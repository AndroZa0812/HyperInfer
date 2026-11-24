# hyperinfer-langchain

[![PyPI](https://img.shields.io/pypi/v/hyperinfer-langchain?style=flat-square)](https://pypi.org/project/hyperinfer-langchain/)
[![Python Versions](https://img.shields.io/pypi/pyversions/hyperinfer-langchain?style=flat-square)](https://pypi.org/project/hyperinfer-langchain/)

LangChain integration for HyperInfer LLM Gateway — wraps `HyperInferClient` as a drop-in LangChain `BaseChatModel`.

## Installation

```bash
pip install hyperinfer-langchain
```

## Usage

### Quick Start (Pythonic)

```python
from hyperinfer_langchain import HyperInferChatModel
from langchain_core.messages import HumanMessage

# Pass API key directly — no Config object needed
llm = HyperInferChatModel(
    model="gpt-4",
    api_key="sk-...",
)

# Non-streaming
response = llm.invoke([HumanMessage(content="Hello!")])
print(response.content)

# Streaming
for chunk in llm.stream([HumanMessage(content="Tell me a joke")]):
    print(chunk.content, end="", flush=True)
```

### Multiple Providers

```python
llm = HyperInferChatModel(
    model="claude-3-5-sonnet-20241022",
    api_keys={
        "openai": "sk-...",
        "anthropic": "sk-ant-...",
    },
)
```

### Advanced Configuration

```python
import asyncio
from hyperinfer import Config
from hyperinfer_langchain import HyperInferChatModel

async def main():
    config = (
        Config()
        .with_api_key("openai", "sk-...")
        .with_alias("fast", "gpt-4o-mini")
    )

    llm = await HyperInferChatModel.from_config(
        config=config,
        model="fast",
        virtual_key="my-team",
    )

    response = llm.invoke([HumanMessage(content="Hello!")])
    print(response.content)

asyncio.run(main())
```

## License

GNU Affero General Public License v3.0 (AGPL-3.0-only)
