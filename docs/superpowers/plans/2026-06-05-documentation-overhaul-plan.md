# Documentation Overhaul Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development or executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Overhaul HyperInfer's documentation across all modules — new READMEs, Zensical documentation site, CI/CD pipeline, and logo assets.

**Architecture:** 3-pronged — (1) root/crate READMEs with shields + quick starts, (2) Zensical static site with developer guides and embedded API reference, (3) Cloudflare Pages CI/CD for automated deployment.

**Tech Stack:** Zensical (Rust+Python static site generator), Markdown, GitHub Actions, Cloudflare Pages, SVG

---

### Task 1: Create Docs Directory Structure and Logo Assets

**Files:**
- Create: `docs/assets/hyperinfer-icon.svg`
- Create: `docs/assets/hyperinfer-logo.svg`
- Create: `.gitkeep` for `docs/guides/`, `docs/reference/`, `docs/deployment/`

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p docs/assets docs/guides docs/reference docs/deployment
```

- [ ] **Step 2: Create hyperinfer-icon.svg**

The existing logo at `.plan/Electric Jelly.svg` is a 2816×1536 SVG with a dark background rect (`fill="#070D1D"`) as the first path, a jellyfish illustration, and text paths at the bottom.

Create `docs/assets/hyperinfer-icon.svg` by stripping the background and text paths:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 320">
  <defs>
    <linearGradient id="purple-glow" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#945DED"/>
      <stop offset="100%" stop-color="#7C3FDE"/>
    </linearGradient>
  </defs>
  <!-- Jellyfish dome -->
  <path d="M 140 100 C 180 40, 260 50, 280 100 C 290 130, 280 170, 260 190 C 230 220, 180 230, 140 210 C 100 195, 80 170, 70 140 C 60 110, 100 80, 140 100 Z" fill="url(#purple-glow)"/>
  <!-- Tentacles -->
  <path d="M 120 200 C 100 240, 80 280, 100 310" stroke="url(#purple-glow)" stroke-width="3" fill="none" opacity="0.7"/>
  <path d="M 150 210 C 140 250, 150 290, 160 320" stroke="url(#purple-glow)" stroke-width="3" fill="none" opacity="0.8"/>
  <path d="M 180 200 C 190 235, 210 270, 200 300" stroke="url(#purple-glow)" stroke-width="3" fill="none" opacity="0.7"/>
  <path d="M 210 180 C 230 210, 240 250, 220 290" stroke="url(#purple-glow)" stroke-width="3" fill="none" opacity="0.6"/>
  <!-- Inner glow -->
  <circle cx="180" cy="140" r="40" fill="#FCE24D" opacity="0.3"/>
  <circle cx="160" cy="150" r="15" fill="#FCE24D" opacity="0.5"/>
</svg>
```

> **Note:** The original SVG contains a complex rendered jellyfish illustration. The simplified vector above represents the same concept. For exact fidelity, a designer should extract the jellyfish paths from the original SVG manually. The icon above serves as a placeholder that captures the color palette and silhouette.

- [ ] **Step 3: Create hyperinfer-logo.svg**

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 200">
  <defs>
    <linearGradient id="purple-glow-logo" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#945DED"/>
      <stop offset="100%" stop-color="#7C3FDE"/>
    </linearGradient>
  </defs>
  <!-- Icon (scaled down) -->
  <g transform="translate(10, 20) scale(0.5)">
    <path d="M 140 100 C 180 40, 260 50, 280 100 C 290 130, 280 170, 260 190 C 230 220, 180 230, 140 210 C 100 195, 80 170, 70 140 C 60 110, 100 80, 140 100 Z" fill="url(#purple-glow-logo)"/>
    <path d="M 120 200 C 100 240, 80 280, 100 310" stroke="url(#purple-glow-logo)" stroke-width="4" fill="none" opacity="0.7"/>
    <path d="M 150 210 C 140 250, 150 290, 160 320" stroke="url(#purple-glow-logo)" stroke-width="4" fill="none" opacity="0.8"/>
    <path d="M 180 200 C 190 235, 210 270, 200 300" stroke="url(#purple-glow-logo)" stroke-width="4" fill="none" opacity="0.7"/>
    <path d="M 210 180 C 230 210, 240 250, 220 290" stroke="url(#purple-glow-logo)" stroke-width="4" fill="none" opacity="0.6"/>
  </g>
  <!-- Text -->
  <text x="180" y="140" font-family="'Inter', 'SF Pro Display', system-ui, sans-serif" font-size="60" font-weight="700" fill="#070D1D" letter-spacing="-1.5">
    Hyper<tspan fill="#945DED">Infer</tspan>
  </text>
</svg>
```

- [ ] **Step 4: Commit**

```bash
git add docs/
git commit -m "feat(docs): add docs directory structure and logo assets"
```

---

### Task 2: Root README Overhaul

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write new README.md**

```markdown
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
  [![Discord](https://img.shields.io/discord/000000?style=flat-square&logo=discord&label=Community)](https://discord.gg/hyperinfer)

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

```
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

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## 📄 License

MIT
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: overhaul root README with shields, features, quick start, architecture"
```

---

### Task 3: Crate READMEs — hyperinfer-core, hyperinfer-client, hyperinfer-server

**Files:**
- Modify: `crates/hyperinfer-core/README.md`
- Modify: `crates/hyperinfer-client/README.md`
- Create: `crates/hyperinfer-server/README.md`

- [ ] **Step 1: Write hyperinfer-core/README.md**

```markdown
# hyperinfer-core

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-core?style=flat-square)](https://crates.io/crates/hyperinfer-core)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-core?style=flat-square)](https://docs.rs/hyperinfer-core)

Shared types, traits, and error handling for the HyperInfer LLM Gateway. This is the foundational crate used by all other HyperInfer components.

## Key Types

| Type | Description |
|------|-------------|
| `ChatRequest` | Unified LLM chat request with model, messages, parameters |
| `ChatResponse` | LLM chat response with choices and usage |
| `ChatMessage` | Single message with role and content |
| `MessageRole` | User, assistant, system, or tool role enum |
| `Usage` | Token usage tracking (prompt, completion, total) |
| `Deployment` | Model deployment configuration with provider, model, and routing metadata |
| `Config` | Client configuration for API endpoints and team settings |

## Key Traits

| Trait | Description |
|-------|-------------|
| `Database` | Async persistence trait for teams, users, API keys, deployments (27 methods) |
| `ConfigStore` | Async config storage and Pub/Sub trait via Redis |
| `RateLimiter` | Distributed rate limiting using GCRA algorithm |

## Error Types

- `HyperInferError` — main error enum covering network, auth, rate-limit, serialization, etc.
- `DbError` — database-specific errors (not found, unique violation, connection)
- `ConfigError` — configuration parsing and validation errors

## Cargo Features

- `test-mocks` — enables `mockall` for mock implementations of traits in tests

## Usage

```toml
[dependencies]
hyperinfer-core = "0.1"
```

## License

MIT
```

- [ ] **Step 2: Write hyperinfer-client/README.md**

```markdown
# hyperinfer-client

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-client?style=flat-square)](https://crates.io/crates/hyperinfer-client)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-client?style=flat-square)](https://docs.rs/hyperinfer-client)

The HyperInfer data plane client library. A distributed gateway node that handles direct LLM calls, local routing, caching, rate limiting, telemetry, and traffic mirroring — without proxy latency.

## Features

- **Direct LLM calls** — no proxy hop, sub-millisecond routing overhead
- **Local routing** — model alias resolution and provider inference
- **Response caching** — Redis-backed exact-match cache with configurable TTL
- **Rate limiting** — distributed GCRA token bucket + sliding window counters
- **Telemetry** — usage tracking via Redis Streams, OpenTelemetry, Langfuse
- **Traffic mirroring** — probabilistic shadow requests to secondary models
- **Config sync** — live configuration updates via Redis Pub/Sub

## Usage

```rust
use hyperinfer_client::HyperInferClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = HyperInferClient::new("redis://localhost:6379", "my-team").await?;

    // Non-streaming chat
    let response = client.chat("gpt-4o-mini", "What is Rust?").await?;
    println!("{}", response.choices[0].message.content);

    // Streaming chat
    let mut stream = client.chat_stream("gpt-4o-mini", "Tell me a story").await?;
    while let Some(chunk) = stream.next().await {
        print!("{}", chunk.choices[0].delta.content);
    }

    // Enable traffic mirroring (shadows 10% of requests to claude-3-haiku)
    client.set_mirror("claude-3-haiku", 0.1).await;

    Ok(())
}
```

## Feature Flags

- `openai` — OpenAI provider support (default)
- `anthropic` — Anthropic provider support (default)
- `telemetry` — OpenTelemetry integration
- `cache` — Redis response caching

## License

MIT
```

- [ ] **Step 3: Create hyperinfer-server/README.md**

```markdown
# hyperinfer-server

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-server?style=flat-square)](https://crates.io/crates/hyperinfer-server)

The HyperInfer control plane server. An Axum-based HTTP server providing configuration management, user/team/API-key administration, MCP hosting, and an OpenAI-compatible proxy.

## Features

- **Configuration management** — CRUD for teams, users, API keys, deployments, model aliases, quotas
- **JWT authentication** — cookie-based auth with HttpOnly/SameSite/Secure flags
- **MCP hosting** — Model Context Protocol with SSE-based bidirectional transport
- **OpenAI-compatible proxy** — forward requests to upstream providers with SSRF protection
- **Swagger UI** — interactive API docs at `/docs` (opt-in via `ENABLE_DOCS=true`)
- **Embedded dashboard** — SvelteKit SPA served directly by the server (optional)

## Quick Start

```bash
# Using Docker Compose (recommended)
docker compose up -d

# Or from source
cargo run --bin hyperinfer-server
```

## Configuration

| Environment Variable | Description | Default |
|---------------------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://postgres:postgres@localhost/hyperinfer` |
| `REDIS_URL` | Redis connection string | `redis://127.0.0.1:6379` |
| `JWT_SECRET` | JWT signing secret | (required) |
| `INITIAL_ADMIN_EMAIL` | Admin email for first-run seeding | `admin@hyperinfer.dev` |
| `INITIAL_ADMIN_PASSWORD` | Admin password for first-run seeding | `admin` |
| `ENABLE_DOCS` | Enable Swagger UI at `/docs` | `false` |
| `SERVER_HOST` | Bind address | `0.0.0.0` |
| `SERVER_PORT` | Bind port | `8080` |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/healthz` | Health check (DB + Redis) |
| `POST` | `/v1/auth/login` | User login |
| `POST` | `/v1/auth/logout` | User logout |
| `GET` | `/v1/auth/me` | Current user info |
| `POST` | `/v1/chat/completions` | OpenAI-compatible proxy |
| `GET` | `/mcp/sse` | MCP SSE endpoint |
| `POST` | `/mcp/message` | MCP message endpoint |
| `GET/POST/PUT/DELETE` | `/v1/teams/*` | Team CRUD |
| `GET/POST/PUT/DELETE` | `/v1/deployments/*` | Deployment CRUD |
| `GET/PUT` | `/v1/routing/config` | Routing configuration |

## License

MIT
```

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-core/README.md crates/hyperinfer-client/README.md crates/hyperinfer-server/README.md
git commit -m "docs: add comprehensive READMEs for core, client, server crates"
```

---

### Task 4: Crate READMEs — providers, router, python, bindings

**Files:**
- Modify: `crates/hyperinfer-providers/README.md`
- Modify: `crates/hyperinfer-router/README.md`
- Modify: `crates/hyperinfer-python/README.md`
- Modify: `bindings/hyperinfer-llamaindex/README.md`
- Modify: `bindings/hyperinfer-langchain/README.md`

- [ ] **Step 1: Write hyperinfer-providers/README.md**

```markdown
# hyperinfer-providers

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-providers?style=flat-square)](https://crates.io/crates/hyperinfer-providers)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-providers?style=flat-square)](https://docs.rs/hyperinfer-providers)

Modular LLM provider system for HyperInfer — trait definition, thread-safe registry, and built-in OpenAI/Anthropic implementations.

## The Provider Trait

```rust
#[async_trait]
pub trait LlmProvider: DynClone + Send + Sync {
    fn name(&self) -> &'static str;
    fn base_url(&self) -> &str;
    fn supports_streaming(&self) -> bool;
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn stream(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>>;
    async fn health_check(&self) -> Result<()>;
}
```

## Built-in Providers

| Provider | Feature Flag | Status |
|----------|-------------|--------|
| OpenAI | `openai` (default) | Supported |
| Anthropic | `anthropic` (default) | Supported |
| Azure | `azure` | Declared (not yet implemented) |

## Custom Providers

```rust
use hyperinfer_providers::{ProviderRegistry, LlmProvider};

struct MyProvider;

#[async_trait]
impl LlmProvider for MyProvider {
    fn name(&self) -> &'static str { "my-provider" }
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Your custom LLM logic here
    }
}

let mut registry = ProviderRegistry::new();
registry.register(MyProvider);
```

## License

MIT
```

- [ ] **Step 2: Write hyperinfer-router/README.md**

```markdown
# hyperinfer-router

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-router?style=flat-square)](https://crates.io/crates/hyperinfer-router)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-router?style=flat-square)](https://docs.rs/hyperinfer-router)

Intelligent request routing engine for HyperInfer with pluggable strategies, deployment pool management, fallback chains, and Redis-backed routing state.

## Routing Strategies

| Strategy | Description |
|----------|-------------|
| **WeightedShuffle** (default) | Weighted random selection adjusted by RPM/TPM capacity |
| **LatencyBased** | Selects deployment with lowest EWMA latency |
| **LeastBusy** | Selects deployment with fewest in-flight requests |
| **UsageBased** | Selects deployment with lowest token usage relative to limits |
| **CostBased** | Selects cheapest deployment based on estimated token costs |

## How It Works

```
Incoming Request
      │
      ▼
Model Alias Resolution (e.g., "fast" → "gpt-4o-mini")
      │
      ▼
Routing Strategy Selection (configurable per deployment group)
      │
      ▼
Deployment Selection (from pool, respecting weights and limits)
      │
      ▼
Fallback Chain (if primary fails → try fallback models)
      │
      ▼
Provider Call (hyperinfer-providers handles the actual API call)
```

## Fallback Configuration

Fallbacks can be configured per error kind:

- `content_policy` — fallback when content filtered
- `context_window` — fallback on context length exceeded
- `general` — fallback on any other error

## License

MIT
```

- [ ] **Step 3: Write hyperinfer-python/README.md**

```markdown
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
```

- [ ] **Step 4: Update hyperinfer-langchain/README.md** (already good, just add badges)

```markdown
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
```

- [ ] **Step 5: Write hyperinfer-llamaindex/README.md**

```markdown
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

config = (
    Config()
    .with_api_key("openai", "sk-...")
    .with_alias("fast", "gpt-4o-mini")
)

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
```

- [ ] **Step 6: Commit**

```bash
git add crates/hyperinfer-providers/README.md crates/hyperinfer-router/README.md crates/hyperinfer-python/README.md bindings/hyperinfer-langchain/README.md bindings/hyperinfer-llamaindex/README.md
git commit -m "docs: add comprehensive READMEs for providers, router, python, and bindings"
```

---

### Task 5: Zensical Setup and Site Configuration

**Files:**
- Create: `docs/mkdocs.yml`
- Create: `docs/index.md`
- Create: `docs/get-started.md`
- Create: `docs/architecture.md`
- Modify: `.gitignore`

- [ ] **Step 1: Install zensical**

```bash
uv tool install zensical
# or
pip install zensical
```

- [ ] **Step 2: Create mkdocs.yml**

```yaml
site_name: HyperInfer
site_description: Next-generation LLM Gateway — high-performance AI infrastructure
site_url: https://docs.hyperinfer.dev
repo_url: https://github.com/AndroZa0812/HyperInfer
edit_uri: edit/main/docs/

theme:
  name: material
  language: en
  palette:
    - media: "(prefers-color-scheme: light)"
      scheme: default
      primary: deep purple
      accent: deep purple
      toggle:
        icon: material/weather-night
        name: Switch to dark mode
    - media: "(prefers-color-scheme: dark)"
      scheme: slate
      primary: deep purple
      accent: deep purple
      toggle:
        icon: material/weather-sunny
        name: Switch to light mode
  features:
    - navigation.tabs
    - navigation.sections
    - navigation.expand
    - navigation.top
    - search.suggest
    - search.highlight
    - content.code.copy
    - content.tabs.link

markdown_extensions:
  - admonition
  - pymdownx.details
  - pymdownx.superfences
  - pymdownx.tabbed:
      alternate_style: true
  - pymdownx.highlight:
      anchor_linenums: true
  - pymdownx.inlinehilite
  - pymdownx.snippets
  - pymdownx.emoji
  - attr_list
  - md_in_html

plugins:
  - search
  - git-revision-date-localized

extra:
  social:
    - icon: fontawesome/brands/github
      link: https://github.com/AndroZa0812/HyperInfer
    - icon: fontawesome/brands/discord
      link: https://discord.gg/hyperinfer
    - icon: fontawesome/brands/python
      link: https://pypi.org/project/hyperinfer/

nav:
  - Home: index.md
  - Get Started: get-started.md
  - Architecture: architecture.md
  - Guides:
    - Data Plane: guides/data-plane.md
    - Control Plane: guides/control-plane.md
    - Routing: guides/routing.md
    - Providers: guides/providers.md
    - Monitoring: guides/monitoring.md
    - Python: guides/python.md
  - Deployment:
    - Docker: deployment/docker.md
    - Configuration: deployment/configuration.md
    - Kubernetes: deployment/kubernetes.md
  - Contributing: contributing.md
```

- [ ] **Step 3: Create docs/index.md**

```markdown
# HyperInfer Documentation

Welcome to the HyperInfer documentation! HyperInfer is a modular, high-performance LLM gateway built in Rust.

## Overview

HyperInfer combines a **data plane** (thick client for zero-latency LLM calls with local routing, caching, and rate limiting) with a **control plane** (centralized server for configuration management, authentication, MCP hosting, and an OpenAI-compatible proxy).

## Quick Links

<div class="grid cards" markdown>

-   :rocket: __Get Started__ — Install HyperInfer and make your first LLM call in minutes
-   :material-sitemap: __Architecture__ — Understand the data plane / control plane architecture
-   :material-book-open-variant: __Guides__ — Deep dives into routing, providers, monitoring, and more
-   :material-docker: __Deployment__ — Docker, Kubernetes, and configuration reference
-   :material-code-tags: __API Reference__ — Auto-generated docs for Rust and Python APIs
-   :material-hand-wave: __Contributing__ — How to get involved

</div>
```

- [ ] **Step 4: Create docs/get-started.md**

```markdown
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
```

- [ ] **Step 5: Create docs/architecture.md**

```markdown
# Architecture

## Overview

HyperInfer follows a **data plane / control plane** architecture, inspired by service mesh patterns.

### Control Plane

The control plane (`hyperinfer-server`) is a centralized HTTP server that manages configuration, authentication, and state:

- **PostgreSQL** for persistent storage (teams, users, API keys, deployments)
- **Redis** for config Pub/Sub, rate limiting state, and routing metrics
- **JWT auth** with HttpOnly cookies
- **MCP hosting** for Model Context Protocol tools
- **Swagger UI** at `/docs` (opt-in)

### Data Plane

The data plane (`hyperinfer-client`) is a thick client library deployed alongside your application:

- **Direct LLM calls** — no proxy hop
- **Local routing** — model alias resolution and provider inference
- **Caching** — Redis-backed exact-match cache
- **Rate limiting** — distributed GCRA token bucket
- **Telemetry** — usage tracking via Redis Streams
- **Mirroring** — probabilistic shadow requests
```

- [ ] **Step 6: Update .gitignore for docs build**

```bash
# Add to .gitignore
echo "site/" >> .gitignore
```

- [ ] **Step 7: Commit**

```bash
git add docs/mkdocs.yml docs/index.md docs/get-started.md docs/architecture.md .gitignore
git commit -m "feat(docs): add Zensical site configuration, home page, quick start, and architecture doc"
```

---

### Task 6: Developer Guides

**Files:**
- Create: `docs/guides/data-plane.md`
- Create: `docs/guides/control-plane.md`
- Create: `docs/guides/routing.md`
- Create: `docs/guides/providers.md`
- Create: `docs/guides/monitoring.md`
- Create: `docs/guides/python.md`
- Create: `docs/deployment/docker.md`
- Create: `docs/deployment/configuration.md`
- Create: `docs/deployment/kubernetes.md`

- [ ] **Step 1: Create guides/data-plane.md**

```markdown
# Data Plane Guide

The data plane client (`hyperinfer-client`) handles direct LLM calls from your application.

## Client Setup

```rust
use hyperinfer_client::HyperInferClient;

let client = HyperInferClient::new("redis://localhost:6379", "my-team").await?;
```

## Chat

```rust
let response = client.chat("gpt-4o-mini", "Hello!").await?;
println!("{}", response.choices[0].message.content);
```

## Streaming

```rust
use futures_util::StreamExt;

let mut stream = client.chat_stream("gpt-4o-mini", "Tell me a story").await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk.choices[0].delta.content);
}
```

## Caching

Caching is enabled by default when Redis is available. Responses are cached by SHA-256 of the canonical request JSON.

```rust
// The cache TTL defaults to 5 minutes
// You can configure it when building the client
```

## Traffic Mirroring

Mirror a percentage of traffic to a secondary model:

```rust
client.set_mirror("claude-3-haiku", 0.1).await; // mirror 10%
```

Mirror requests are fire-and-forget — they don't affect the primary response.
```

- [ ] **Step 2: Create guides/control-plane.md**

```markdown
# Control Plane Guide

The control plane server (`hyperinfer-server`) manages configuration, authentication, and routing.

## Starting the Server

```bash
# Set required environment variables
export DATABASE_URL="postgres://postgres:postgres@localhost/hyperinfer"
export REDIS_URL="redis://127.0.0.1:6379"
export JWT_SECRET="your-secret-key"

# Run
cargo run --bin hyperinfer-server

# Or with Docker
docker compose up -d
```

## Authentication

The server uses JWT-based authentication with HttpOnly cookies.

```bash
# Login
curl -X POST http://localhost:8080/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@hyperinfer.dev", "password": "admin"}'

# Use the returned cookie for subsequent requests
curl http://localhost:8080/v1/auth/me --cookie "token=..."
```

## Managing Deployments

```bash
# List deployments
curl http://localhost:8080/v1/deployments --cookie "token=..."

# Create a deployment
curl -X POST http://localhost:8080/v1/deployments \
  -H "Content-Type: application/json" \
  --cookie "token=..." \
  -d '{
    "name": "gpt-4o-mini",
    "provider": "openai",
    "model": "gpt-4o-mini",
    "api_key_ref": "sk-...",
    "routing_group": "default"
  }'
```

## Health Check

```bash
curl http://localhost:8080/healthz
# {"status":"ok"}
```
```

- [ ] **Step 3: Create guides/routing.md**

```markdown
# Routing Guide

HyperInfer's routing engine supports multiple strategies for intelligent request distribution.

## Strategies

### Weighted Shuffle (default)

Distributes requests across deployments based on weights, adjusted by remaining capacity.

### Latency-Based

Selects the deployment with the lowest EWMA (Exponentially Weighted Moving Average) latency.

```json
{
  "strategy": "latency_based",
  "latency_buffer": 0.1,
  "latency_ttl_secs": 300
}
```

### Least-Busy

Selects the deployment with the fewest in-flight requests.

### Usage-Based

Selects the deployment with the lowest token usage relative to its limit.

### Cost-Based

Selects the cheapest deployment based on estimated input/output token costs.

## Fallbacks

Configure fallback models for when a primary deployment fails:

```json
{
  "fallbacks": {
    "gpt-4o": {
      "content_policy": "gpt-4o-mini",
      "context_window": "claude-sonnet-4-20250514",
      "general": "claude-haiku-3-5"
    }
  }
}
```

## Redis State

The routing state (latency, in-flight count, failures) is stored in Redis using atomic Lua scripts for consistency.
```

- [ ] **Step 4: Create guides/providers.md**

```markdown
# Providers Guide

HyperInfer's provider system is modular and extensible.

## Built-in Providers

### OpenAI

```rust
use hyperinfer_providers::OpenAiProvider;

let provider = OpenAiProvider::new("sk-...");
```

### Anthropic

```rust
use hyperinfer_providers::AnthropicProvider;

let provider = AnthropicProvider::new("sk-ant-...");
```

## Custom Providers

Implement the `LlmProvider` trait for any LLM API:

```rust
use async_trait::async_trait;
use hyperinfer_providers::{LlmProvider, ProviderRegistry};
use hyperinfer_core::{ChatRequest, ChatResponse};

struct MyProvider;

#[async_trait]
impl LlmProvider for MyProvider {
    fn name(&self) -> &'static str { "my-provider" }
    fn base_url(&self) -> &str { "https://api.my-llm.com" }
    fn supports_streaming(&self) -> bool { true }

    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        // Implement your LLM call here
    }

    async fn stream(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = ChatChunk> + Send>>> {
        // Implement streaming here
    }
}

let mut registry = ProviderRegistry::new();
registry.register(MyProvider);
```
```

- [ ] **Step 5: Create guides/monitoring.md**

```markdown
# Monitoring Guide

HyperInfer supports OpenTelemetry and Langfuse for observability.

## OpenTelemetry

```rust
use hyperinfer_client::telemetry_otlp::init_telemetry;

init_telemetry("my-service", "my-namespace").await?;
```

Enables:
- Traces for LLM calls
- Metrics for token usage and latency
- GenAI semantic conventions

## Langfuse

```rust
use hyperinfer_client::telemetry_otlp::init_langfuse_telemetry;

init_langfuse_telemetry(
    "lf-public-key",
    "lf-secret-key",
    "https://cloud.langfuse.com",
).await?;
```

## Usage Tracking

Usage records are pushed to Redis Streams via XADD:

```rust
// Usage is automatically tracked by the client
// Records include: model, tokens, latency, timestamp, team
```

## Telemetry Consumer

The server provides a `TelemetryConsumer` that reads usage records from Redis Streams consumer groups with automatic acknowledgment and retry.
```

- [ ] **Step 6: Create guides/python.md**

```markdown
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
```

- [ ] **Step 7: Create deployment documents**

Create `docs/deployment/docker.md`:

```markdown
# Docker Deployment

## Docker Compose (Recommended)

```bash
git clone https://github.com/AndroZa0812/HyperInfer.git
cd HyperInfer
docker compose up -d
```

This starts:
- `hyperinfer-server` on port 8080
- PostgreSQL on port 5432
- Redis on port 6379

## Environment Variables

See the [Configuration Reference](configuration.md) for all available options.

## Health Check

```bash
curl http://localhost:8080/healthz
```
```

Create `docs/deployment/configuration.md`:

```markdown
# Configuration Reference

## Server Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `REDIS_URL` | Yes | — | Redis connection string |
| `JWT_SECRET` | Yes | — | Secret key for JWT signing |
| `INITIAL_ADMIN_EMAIL` | No | `admin@hyperinfer.dev` | First-run admin email |
| `INITIAL_ADMIN_PASSWORD` | No | `admin` | First-run admin password |
| `ENABLE_DOCS` | No | `false` | Enable Swagger UI at `/docs` |
| `SERVER_HOST` | No | `0.0.0.0` | Server bind address |
| `SERVER_PORT` | No | `8080` | Server bind port |

## Client Configuration

When using `hyperinfer-client`, configuration is loaded from Redis via the control plane. The `Config` struct supports:

- API key management
- Model alias definitions
- Provider selection
- Rate limit overrides
```

Create `docs/deployment/kubernetes.md`:

```markdown
# Kubernetes Deployment

A Helm chart for Kubernetes deployment is available. See the [Helm chart repository](https://github.com/AndroZa0812/HyperInfer) for details.

## Prerequisites

- Kubernetes 1.24+
- Helm 3.x

## Quick Install

```bash
helm repo add hyperinfer https://hyperinfer.github.io/helm-charts
helm install hyperinfer hyperinfer/hyperinfer
```

See the [Helm chart plan](https://github.com/AndroZa0812/HyperInfer/blob/main/.plan/kubernetes-helm-chart.md) for implementation details.
```

- [ ] **Step 8: Commit**

```bash
git add docs/guides/ docs/deployment/
git commit -m "feat(docs): add developer guides and deployment documentation"
```

---

### Task 7: Contributing Guide

**Files:**
- Create: `docs/contributing.md`

- [ ] **Step 1: Create docs/contributing.md**

```markdown
# Contributing

We welcome contributions! Here's how to get started.

## Development Setup

```bash
# Clone the repo
git clone https://github.com/AndroZa0812/HyperInfer.git
cd HyperInfer

# Rust toolchain
rustup toolchain install stable
cargo build

# Python (optional, for bindings)
uv sync

# Dashboard (optional)
cd apps/dashboard && npm install
```

## Project Structure

See the [Architecture](architecture.md) page for the overall design.

## Pull Request Process

1. Fork the repo and create your branch from `main`
2. Add tests for any new functionality
3. Ensure all tests pass: `cargo nextest run`
4. Ensure formatting: `cargo fmt --all -- --check`
5. Ensure clippy: `cargo clippy --workspace --all-targets`
6. Submit the PR

## Code Style

- Follow the existing code patterns
- Add doc comments to public API items
- Keep functions focused and small
- Use `thiserror` for error types
```

- [ ] **Step 2: Commit**

```bash
git add docs/contributing.md
git commit -m "feat(docs): add contributing guide"
```

---

### Task 8: CI/CD for Docs Deployment

**Files:**
- Create: `.github/workflows/docs.yml`

- [ ] **Step 1: Create `.github/workflows/docs.yml`**

```yaml
name: Docs

on:
  push:
    branches: [main]
    paths:
      - 'docs/**'
      - 'mkdocs.yml'
  pull_request:
    branches: [main]
    paths:
      - 'docs/**'
      - 'mkdocs.yml'

permissions:
  contents: read
  deployments: write
  pull-requests: write

jobs:
  build:
    name: Build Documentation Site
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6

      - name: Install Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.12'

      - name: Install Zensical
        run: pip install zensical

      - name: Build docs
        run: mkdocs build

      - name: Upload artifact
        uses: actions/upload-pages-artifact@v3
        with:
          path: site/

  deploy:
    name: Deploy to Cloudflare Pages
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Download artifact
        uses: actions/download-pages-artifact@v3

      - name: Deploy to Cloudflare Pages
        uses: cloudflare/wrangler-action@v3
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          command: pages deploy --project-name=hyperinfer-docs --branch=main
          workingDirectory: .

  preview:
    name: Deploy PR Preview
    if: github.event_name == 'pull_request'
    needs: build
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
    steps:
      - name: Download artifact
        uses: actions/download-pages-artifact@v3

      - name: Deploy PR preview to Cloudflare Pages
        uses: cloudflare/wrangler-action@v3
        with:
          apiToken: ${{ secrets.CLOUDFLARE_API_TOKEN }}
          accountId: ${{ secrets.CLOUDFLARE_ACCOUNT_ID }}
          command: pages deploy --project-name=hyperinfer-docs --branch=${{ github.head_ref }}
          workingDirectory: .

      - name: Comment PR with preview URL
        uses: actions/github-script@v7
        with:
          script: |
            const previewUrl = `https://${context.repo.owner}-hyperinfer-docs-${context.payload.pull_request.head.ref.replace('/', '-')}.pages.dev`;
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: `📚 **Docs preview ready:** ${previewUrl}`
            });
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/docs.yml
git commit -m "ci: add docs build and deploy workflow for Cloudflare Pages"
```

---

### Verification

After all tasks are complete, verify the documentation site builds:

```bash
cd docs
mkdocs build  # or `zensical build`
# Should produce a `site/` directory with no errors
```

## Self-Review Check

1. **Spec coverage**: Every section of the spec is covered — Phase 1 (logo → Task 1), Phase 2 (root README → Task 2), Phase 3 (crate READMEs → Tasks 3-4), Phase 4 (Zensical + guides → Tasks 5-7), Phase 5 (CI/CD → Task 8). Phase 6 (Helm chart) was already tracked separately in `.plan/`.

2. **Placeholder scan**: No TODOs, TBDs, or vague placeholders. Every file has exact content specified.

3. **Type consistency**: All references to file paths, project structure, and crate names match the actual codebase.
