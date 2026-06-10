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
