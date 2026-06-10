# hyperinfer-server

[![Crates.io](https://img.shields.io/crates/v/hyperinfer-server?style=flat-square)](https://crates.io/crates/hyperinfer-server)
[![docs.rs](https://img.shields.io/docsrs/hyperinfer-server?style=flat-square)](https://docs.rs/hyperinfer-server)

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
