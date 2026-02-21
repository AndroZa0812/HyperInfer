# HyperInfer - Next-Generation LLM Gateway
![CodeRabbit Pull Request Reviews](https://img.shields.io/coderabbit/prs/github/AndroZa0812/HyperInfer?utm_source=oss&utm_medium=github&utm_campaign=AndroZa0812%2FHyperInfer&labelColor=171717&color=FF570A&link=https%3A%2F%2Fcoderabbit.ai&label=CodeRabbit+Reviews)

A modular Rust & SvelteKit monorepo for building high-performance AI infrastructure.

## Project Structure

```
hyperinfer/
├── Cargo.toml              # Workspace definition
├── crates/
│   ├── hyperinfer-core     # Shared types, traits, and error handling
│   ├── hyperinfer-client   # Data Plane thick client library
│   ├── hyperinfer-server   # Control Plane server binary
│   └── hyperinfer-python   # Python bindings via PyO3
├── apps/
│   └── dashboard           # SvelteKit Admin UI (compiled to static assets)
└── docs/
    └── phase1_plan.md      # Implementation plan for Phase 1
```

## Crates

### hyperinfer-core
Shared data structures, error handling, and utilities used across the monorepo.

### hyperinfer-client  
The thick client library that acts as a distributed gateway node. It handles direct LLM calls, local routing, and rate limiting without proxy latency.

### hyperinfer-server
The centralized control plane that manages configuration, stateful conversations, and MCP hosting.

### hyperinfer-python
PyO3 bindings to expose the Rust Data Plane functionality to Python environments.

## Implementation Status

This is Phase 1 implementation which includes:
- Core crate with shared types and error handling
- Client library structure with Redis Pub/Sub support  
- Server binary skeleton
- Python integration stubs
- Rate limiting infrastructure