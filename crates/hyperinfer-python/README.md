# hyperinfer-python

[![PyPI](https://img.shields.io/pypi/v/hyperinfer?style=flat-square)](https://pypi.org/project/hyperinfer/)
[![Python Versions](https://img.shields.io/pypi/pyversions/hyperinfer?style=flat-square)](https://pypi.org/project/hyperinfer/)

Native Python bindings for HyperInfer — wraps the Rust data plane client via PyO3.

## Installation

```bash
pip install hyperinfer
```

## Usage

### Quick Start (Pythonic)

```python
import asyncio
from hyperinfer import Client

async def main():
    # Pass API keys directly — no Config object needed
    client = Client(
        api_keys={"openai": "sk-..."},
        default_provider="openai",
    )

    # Omit the key — uses the client's virtual_key
    response = await client.chat(
        model="gpt-4",
        messages=[{"role": "user", "content": "What is HyperInfer?"}]
    )
    print(response)

    await client.close()

asyncio.run(main())
```

### From Environment Variables

```python
from hyperinfer import Client

# Reads OPENAI_API_KEY, ANTHROPIC_API_KEY, GEMINI_API_KEY automatically
client = Client()
```

### Streaming

```python
async for chunk in client.stream(
    model="gpt-4",
    messages=[{"role": "user", "content": "Tell me a story"}]
):
    print(chunk["delta"], end="", flush=True)
```

### Advanced Configuration

```python
from hyperinfer import Config, Client

# Use Config for complex setups with aliases, quotas, routing
config = (
    Config()
    .with_api_key("openai", "sk-...")
    .with_api_key("anthropic", "sk-ant-...")
    .with_alias("fast", "gpt-4o-mini")
    .with_alias("smart", "claude-3-5-sonnet-20241022")
    .with_quota("default", rpm=60, tpm=100000)
)

client = Client(config=config)
```

### Tool Calling

```python
response = await client.chat(
    model="gpt-4",
    messages=[{"role": "user", "content": "What's the weather in NYC?"}],
    tools=[{
        "type": "function",
        "function": {
            "name": "get_weather",
            "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
        }
    }]
)
```

## License

GNU Affero General Public License v3.0 (AGPL-3.0-only)
