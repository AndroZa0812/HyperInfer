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
