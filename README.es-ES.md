

<div align="center">

  <img src="docs/assets/hyperinfer-icon.svg" width="120" alt="HyperInfer">

  # HyperInfer

  **La pasarela LLM de código abierto para infraestructura de IA de alto rendimiento.**

   [![GitHub Stars](https://img.shields.io/github/stars/AndroZa0812/HyperInfer?style=flat-square&logo=github)](https://github.com/AndroZa0812/HyperInfer)
   [![Crates.io](https://img.shields.io/crates/v/hyperinfer-core?style=flat-square&logo=rust)](https://crates.io/crates/hyperinfer-core)
   [![PyPI](https://img.shields.io/pypi/v/hyperinfer?style=flat-square&logo=pypi)](https://pypi.org/project/hyperinfer/)
   [![CI](https://img.shields.io/github/actions/workflow/status/AndroZa0812/HyperInfer/rust-ci.yml?style=flat-square&logo=githubactions)](https://github.com/AndroZa0812/HyperInfer/actions)
   [![License](https://img.shields.io/github/license/AndroZa0812/HyperInfer?style=flat-square)](LICENSE)

</div>

HyperInfer es una pasarela modular de alto rendimiento para infraestructura de LLM. Combina un **plano de datos** (biblioteca de cliente completo para llamadas LLM con latencia cero) con un **plano de control** (servidor centralizado para configuración, enrutamiento y alojamiento de MCP).

Construido en Rust para garantizar rendimiento y seguridad, con enlaces de Python para flujos de trabajo de ML.

## ✨ Características

| Característica | Descripción |
|---------|-------------|
| **Plano de datos** | Nodo de pasarela distribuido: llamadas directas a LLM, enrutamiento local, limitación de tasa y almacenamiento en caché. Sin latencia de proxy. |
| **Plano de control** | Servidor centralizado para la gestión de equipos/usuarios/autenticación, sincronización de configuración y alojamiento de MCP. |
| **Enrutamiento inteligente** | Estrategias de enrutamiento intercambiables: mezcla ponderada, basada en latencia, menor carga, basada en uso, basada en costo. |
| **Multi-proveedor** | Soporte integrado para OpenAI y Anthropic. Proveedores personalizados mediante un registro basado en traits. |
| **Enlaces de Python** | Enlaces nativos de PyO3 con una API estilo Python. Integraciones con LangChain y LlamaIndex. |
| **Observabilidad** | Integración con OpenTelemetry y Langfuse para trazabilidad, métricas y seguimiento de uso. |
| **Limitación de tasa** | Limitación de tasa distribuida vía GCRA (bucket de tokens) con Redis. |

## 🚀 Inicio rápido

### Rust (Biblioteca del cliente)

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

### Servidor (Docker Compose)

```bash
git clone https://github.com/AndroZa0812/HyperInfer.git
cd HyperInfer
docker compose up -d
# Server runs at http://localhost:8080
# Dashboard at http://localhost:8080/dashboard
```

## 📦 Arquitectura

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

## 📚 Documentación

| Recurso | Enlace |
|----------|------|
| Sitio de documentación | [docs.hyperinfer.dev](https://docs.hyperinfer.dev) |
| Referencia de la API de Rust | [docs.rs/hyperinfer-core](https://docs.rs/hyperinfer-core) |
| Paquete de Python | [pypi.org/project/hyperinfer](https://pypi.org/project/hyperinfer) |

## 🧩 Estructura del proyecto

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

## 🤝 Contribuciones

¡Agradecemos las contribuciones! Consulta la [guía de contribución](docs/contributing.md) en el sitio de documentación.

## 📄 Licencia

MIT
