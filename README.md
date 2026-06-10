<div align="center">

  <img src="docs/assets/hyperinfer-icon.svg" width="120" alt="HyperInfer">

  # HyperInfer

  **The open-source LLM gateway for high-performance AI infrastructure.**

  [![GitHub Stars](https://img.shields.io/github/stars/AndroZa0812/HyperInfer?style=flat-square&logo=github)](https://github.com/AndroZa0812/HyperInfer)
  [![Crates.io](https://img.shields.io/crates/v/hyperinfer-core?style=flat-square&logo=rust)](https://crates.io/crates/hyperinfer-core)
  [![PyPI](https://img.shields.io/pypi/v/hyperinfer?style=flat-square&logo=pypi)](https://pypi.org/project/hyperinfer/)
  [![CI](https://img.shields.io/github/actions/workflow/status/AndroZa0812/HyperInfer/rust-ci.yml?style=flat-square&logo=githubactions)](https://github.com/AndroZa0812/HyperInfer/actions)
  [![License](https://img.shields.io/github/license/AndroZa0812/HyperInfer?style=flat-square)](LICENSE)
  [![CodeRabbit](https://img.shields.io/coderabbit/prs/github/AndroZa0812/HyperInfer?style=flat-square&labelColor=171717&color=FF570A&label=CodeRabbit)](https://coderabbit.ai)
  [![Community](https://img.shields.io/badge/-Community-5865F2?style=flat-square&logo=discord&logoColor=white)](https://discord.gg/hyperinfer)

</div>

HyperInfer is a modular, high-performance gateway for LLM infrastructure. It combines a **data plane** (thick client library for zero-latency LLM calls) with a **control plane** (centralized server for configuration, routing, and MCP hosting).

Built in Rust for performance and safety, with Python bindings for ML workflows.

## ✨ Features

| Feature | Description |
|---------|-------------|
| **Data Plane** | Distributed gateway node — direct LLM calls, local routing, rate limiting, caching. No proxy latency. |
| **Control Plane** | Centralized server for team/user/auth management, config sync, MCP hosting. |
| **Intelligent Routing** | Pluggable routing strategies — weighted shuffle, latency-based, least-busy, usage-based, cost-based. |
| **Multi-Provider** | Built-in OpenAI and Anthropic support. Custom providers via a trait-based registry. |
| **Python Bindings** | Native PyO3 bindings with a Pythonic API. LangChain and LlamaIndex integrations. |
| **Observability** | OpenTelemetry + Langfuse integration for tracing, metrics, and usage tracking. |
| **Rate Limiting** | Distributed rate limiting via GCRA (token bucket) with Redis. |

## 🚀 Quick Start

### Rust (Client Library)

```bash
cargo add hyperinfer-client
```

```rust
use hyperinfer_client::HyperInferClient;

let client = HyperInferClient::new("config_endpoint", "my_team").await?;
let response = client.chat("gpt-4o-mini", "Hello!").await?;
println!("{}", response.choices[0].message.content);
```

### Python

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

### Server (Docker Compose)

```bash
git clone https://github.com/AndroZa0812/HyperInfer.git
cd HyperInfer
docker compose up -d
# Server runs at http://localhost:8080
# Dashboard at http://localhost:8080/dashboard
```

## 📦 Architecture

```text
┌─────────────────────────────────────────────────────┐
│                   Control Plane                      │
│  ┌──────────────────────────────────────────────┐   │
│  │         hyperinfer-server (Axum)             │   │
│  │  Auth │ Config │ MCP │ Proxy │ Dashboard     │   │
│  └──────┬───────────────────────────────────────┘   │
└─────────┼───────────────────────────────────────────┘
          │ Config sync (Redis Pub/Sub)
┌─────────┼───────────────────────────────────────────┐
│  ┌──────┴───────────────────────────────────────┐   │
│  │         hyperinfer-client                     │   │
│  │  Router │ Cache │ Telemetry │ Mirroring      │   │
│  └──────┬───────────────────────────────────────┘   │
│         │                                            │
│  ┌──────┴───────────────────────────────────────┐   │
│  │    LLM Providers (OpenAI, Anthropic, ...)     │   │
│  └──────────────────────────────────────────────┘   │
│                   Data Plane                         │
└─────────────────────────────────────────────────────┘
```

## 📚 Documentation

| Resource | Link |
|----------|------|
| Documentation Site | [docs.hyperinfer.dev](https://docs.hyperinfer.dev) |
| Rust API Reference | [docs.rs/hyperinfer-core](https://docs.rs/hyperinfer-core) |
| Python Package | [pypi.org/project/hyperinfer](https://pypi.org/project/hyperinfer) |

## 🧩 Project Structure

```
hyperinfer/
├── crates/
│   ├── hyperinfer-core       # Shared types, traits, error handling
│   ├── hyperinfer-client     # Data plane client library
│   ├── hyperinfer-server     # Control plane server binary
│   ├── hyperinfer-providers  # LLM provider trait + OpenAI/Anthropic
│   ├── hyperinfer-router     # Intelligent request routing engine
│   └── hyperinfer-python     # PyO3 bindings
├── bindings/
│   ├── hyperinfer-langchain  # LangChain integration
│   └── hyperinfer-llamaindex # LlamaIndex integration
├── apps/
│   └── dashboard             # SvelteKit admin UI
└── docs/                     # Documentation site (Zensical)
```

## 🤝 Contributing

We welcome contributions! See the [contributing guide](docs/contributing.md) on the documentation site.

## 📄 License

MIT
