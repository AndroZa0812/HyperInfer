# Python Guide

HyperInfer provides native Python bindings through PyO3.

## Installation

```bash
pip install hyperinfer
```

## Basic Usage

```python
from hyperinfer import Config, HyperInferClient

config = Config() \
    .with_api_key("openai", "sk-...") \
    .with_alias("fast", "gpt-4o-mini")

client = HyperInferClient(config)

# Non-streaming
response = client.chat("fast", "Hello!")
print(response)

# Streaming
for chunk in client.chat_stream("fast", "Tell me a story"):
    print(chunk, end="", flush=True)
```

## LangChain Integration

```bash
pip install hyperinfer-langchain
```

```python
from hyperinfer import Config
from hyperinfer_langchain import HyperInferChatModel

llm = await HyperInferChatModel.from_config(
    config=config,
    model="fast",
    virtual_key="my-team",
)
```

## LlamaIndex Integration

```bash
pip install hyperinfer-llamaindex
```

```python
from hyperinfer_llamaindex import HyperInferLLM

llm = HyperInferLLM.from_config(
    config=config,
    model="fast",
)
```
