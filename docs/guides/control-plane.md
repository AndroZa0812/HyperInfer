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
