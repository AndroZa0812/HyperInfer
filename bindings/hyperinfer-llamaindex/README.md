# hyperinfer-llamaindex

[![PyPI](https://img.shields.io/pypi/v/hyperinfer-llamaindex?style=flat-square)](https://pypi.org/project/hyperinfer-llamaindex/)
[![Python Versions](https://img.shields.io/pypi/pyversions/hyperinfer-llamaindex?style=flat-square)](https://pypi.org/project/hyperinfer-llamaindex/)

LlamaIndex integration for HyperInfer LLM Gateway — wraps `HyperInferClient` as a LlamaIndex `CustomLLM`.

## Installation

```bash
pip install hyperinfer-llamaindex
```

## Usage

```python
from hyperinfer import Config
from hyperinfer_llamaindex import HyperInferLLM

config = Config().with_api_key("openai", "sk-...").with_alias("fast", "gpt-4o-mini")

llm = HyperInferLLM.from_config(
    config=config,
    model="fast",
)

# Completion
response = llm.complete("Hello!")
print(response.text)

# Streaming
for chunk in llm.stream_complete("Tell me a story"):
    print(chunk.text, end="", flush=True)
```

## License

MIT
