# Contributing to HyperInfer

First off, thanks for taking the time to contribute! 🎉

## Code of Conduct

This project and everyone participating in it is governed by the [HyperInfer Code of Conduct](https://github.com/AndroZa0812/HyperInfer/blob/main/CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How to Contribute

### Reporting Bugs

Before creating bug reports, please check the issue list to avoid duplicates. When creating a bug report, include as many details as possible:

- A clear and descriptive title
- Steps to reproduce the behavior
- Expected behavior vs actual behavior
- Environment details (OS, Rust version, Python version)
- Logs or stack traces
- A minimal reproducible example

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. Provide:

- A clear title and description
- Step-by-step description of the suggested enhancement
- Specific examples of how it would work
- Why this would be useful to the project

### Pull Requests

1. Fork the repository and create a feature branch from `main`
2. If you've added code, add tests that cover your changes
3. Ensure all existing and new tests pass
4. Run `cargo clippy` and address any warnings
5. Run `cargo fmt` to format your code
6. Update documentation as needed
7. Ensure your PR description clearly describes the problem and solution

## Development Setup

### Prerequisites

- Rust 1.75+
- Python 3.10+
- Docker and Docker Compose

### Getting Started

```bash
git clone https://github.com/AndroZa0812/HyperInfer.git
cd HyperInfer

# Start infrastructure
docker compose up -d postgres redis

# Run tests
cargo test

# Run the server
cargo run --bin hyperinfer-server
```

### Project Structure

```
src/                    # Cargo workspace
├── hyperinfer-core/    # Core types and shared logic
├── hyperinfer-client/  # SDK client library
├── hyperinfer-server/  # Control plane server
├── hyperinfer-router/  # LLM request router
├── hyperinfer-providers/  # LLM provider implementations
└── hyperinfer-python/  # Python bindings (PyO3)
```

## Code Style

- **Rust**: Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- **Python**: Follow [PEP 8](https://peps.python.org/pep-0008/) and use type hints
- **Naming**: Descriptive names, avoid abbreviations
- **Documentation**: Document public APIs with doc comments (`///`)

## Testing

- Unit tests should be placed alongside the code they test (standard Rust convention)
- Integration tests go in `tests/` directories
- Python tests use `pytest` and live in `tests/` alongside Python source
- Run `cargo test` before submitting a PR

## Documentation

- New features should include documentation
- Update existing docs when changing behavior
- Doc comments (`///`) are preferred for Rust public API documentation
- This documentation site uses Zensical/MkDocs with Markdown
