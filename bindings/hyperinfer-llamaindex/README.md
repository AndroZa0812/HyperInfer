# hyperinfer-llamaindex

[![PyPI](https://img.shields.io/pypi/v/hyperinfer-llamaindex?style=flat-square)](https://pypi.org/project/hyperinfer-llamaindex/)
[![Python Versions](https://img.shields.io/pypi/pyversions/hyperinfer-llamaindex?style=flat-square)](https://pypi.org/project/hyperinfer-llamaindex/)

LlamaIndex integration for HyperInfer LLM Gateway — wraps `HyperInferClient` as a LlamaIndex `CustomLLM`.

## Installation

```bash
pip install hyperinfer-llamaindex
```

## Usage

### Quick Start (Pythonic)

```python
from hyperinfer_llamaindex import HyperInferLLM

# Pass API key directly — no Config object needed
llm = HyperInferLLM(
    model="gpt-4",
    api_key="sk-...",
)

# Completion
response = llm.complete("Hello!")
print(response.text)

# Streaming
for chunk in llm.stream_complete("Tell me a story"):
    print(chunk.delta, end="", flush=True)
```

### Multiple Providers

```python
llm = HyperInferLLM(
    model="claude-3-5-sonnet-20241022",
    api_keys={
        "openai": "sk-...",
        "anthropic": "sk-ant-...",
    },
)
```

### Advanced Configuration

```python
from hyperinfer import Config
from hyperinfer_llamaindex import HyperInferLLM

config = (
    Config()
    .with_api_key("openai", "sk-...")
    .with_alias("fast", "gpt-4o-mini")
)

llm = HyperInferLLM.from_config(
    config=config,
    model="fast",
)

response = llm.complete("Hello!")
print(response.text)
```

## License

GNU Affero General Public License v3.0 (AGPL-3.0-only)
