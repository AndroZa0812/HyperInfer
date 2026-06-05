# hyperinfer-python

[![PyPI](https://img.shields.io/pypi/v/hyperinfer?style=flat-square)](https://pypi.org/project/hyperinfer/)
[![Python Versions](https://img.shields.io/pypi/pyversions/hyperinfer?style=flat-square)](https://pypi.org/project/hyperinfer/)

Native Python bindings for HyperInfer — wraps the Rust data plane client via PyO3.

## Installation

```bash
pip install hyperinfer
```

## Usage

```python
from hyperinfer import Config, HyperInferClient

# Configure with your API keys
config = (
    Config()
    .with_api_key("openai", "sk-...")
    .with_api_key("anthropic", "sk-ant-...")
    .with_alias("fast", "gpt-4o-mini")
    .with_alias("smart", "claude-sonnet-4-20250514")
)

# Create the client
client = HyperInferClient(config)

# Non-streaming chat
response = client.chat("fast", "What is HyperInfer?")
print(response)

# Streaming chat
for chunk in client.chat_stream("smart", "Tell me a story"):
    print(chunk, end="", flush=True)
```

## Custom Python Providers

```python
from hyperinfer import ProviderRegistry

def my_custom_provider(request):
    # Your custom LLM logic
    return {"content": "Hello from Python!"}

registry = ProviderRegistry()
registry.register_provider("my-provider", my_custom_provider)
client = HyperInferClient(config, registry)
```

## License

MIT
