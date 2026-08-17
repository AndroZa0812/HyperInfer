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
import asyncio
from hyperinfer import Config, Client


async def main():
    # Configure with your API keys
    config = (
        Config()
        .with_api_key("openai", "sk-...")
        .with_api_key("anthropic", "sk-ant-...")
        .with_alias("fast", "gpt-4o-mini")
        .with_alias("smart", "claude-3-5-sonnet-20241022")
    )

    # Create the client (async wrapper)
    client = Client(redis_url="redis://localhost:6379", config=config)

    # Non-streaming chat
    response = await client.chat(
        key="my-key", model="fast", messages=[{"role": "user", "content": "What is HyperInfer?"}]
    )
    print(response)

    # Streaming chat
    async for chunk in client.stream(
        key="my-key", model="smart", messages=[{"role": "user", "content": "Tell me a story"}]
    ):
        print(chunk["delta"], end="", flush=True)

    await client.close()


asyncio.run(main())
```

## License

MIT
