# Get Started

## Rust

```bash
cargo add hyperinfer-client
```

```rust
use hyperinfer_client::HyperInferClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = HyperInferClient::new("redis://localhost:6379", "my-team").await?;
    let response = client.chat("gpt-4o-mini", "Hello!").await?;
    println!("{}", response.choices[0].message.content);
    Ok(())
}
```

## Python

```bash
pip install hyperinfer
```

```python
from hyperinfer import Config, HyperInferClient

config = Config().with_api_key("openai", "sk-...")
client = HyperInferClient(config)
response = client.chat("gpt-4o-mini", "Hello!")
print(response)
```

## Docker

```bash
docker compose up -d
# Server running at http://localhost:8080
# Dashboard at http://localhost:8080/dashboard
```
