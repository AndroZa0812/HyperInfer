# Control Plane Guide

The control plane server (`hyperinfer-server`) manages configuration, authentication, and routing.

!!! info "Terminology"
    The terms **Control Plane** and **Server** are used interchangeably throughout this documentation — they refer to the same thing. Similarly, **Data Plane** and **Client** are synonyms. We use "Control Plane"/"Data Plane" in architecture discussions and "Server"/"Client" when talking about specific code or binaries.

## 1. What is the Control Plane?

The control plane is the **administrative brain** of HyperInfer. While the data plane focuses on low-latency request forwarding, the control plane focuses on:

*   **Configuration Management**: Defining deployments, model aliases, routing rules, and quotas.
*   **Authentication & Authorization**: Managing teams, users, and API keys.
*   **Chat Completions**: Proxying chat requests to the routing engine (`/v1/chat/completions`).
*   **Observability**: Exposing routing health and config sync endpoints.

!!! tip "Separation of Concerns"
    The control plane is **not** in the critical path of LLM requests. A slow or briefly unavailable control plane will not interrupt your running traffic — the data plane caches the routing config in Redis and continues to serve requests independently.

---

## 2. Starting the Server

The control plane requires a PostgreSQL database (for persistent config) and a Redis instance (for shared state).

### 2.1. Required Environment Variables

| Variable | Required | Description |
| :--- | :--- | :--- |
| `DATABASE_URL` | Yes | PostgreSQL connection string (e.g., `postgres://user:pass@localhost/hyperinfer`). |
| `REDIS_URL` | Yes | Redis connection string (e.g., `redis://127.0.0.1:6379`). |
| `JWT_SECRET` | Yes | Secret key for signing authentication tokens. |
| `INITIAL_ADMIN_EMAIL` | No | If set, seeds an initial admin user on first startup. |
| `INITIAL_ADMIN_PASSWORD` | No | Password for the seeded admin user. |

### 2.2. Starting Methods

=== "Docker Compose"

    The fastest way to get started. Spins up PostgreSQL, Redis, and the server in one command.

    ```bash
    docker compose up -d
    ```

=== "Cargo"

    Run from source for local development:

    ```bash
    export DATABASE_URL="postgres://postgres:postgres@localhost/hyperinfer"
    export REDIS_URL="redis://127.0.0.1:6379"
    export JWT_SECRET="your-secret-key"

    cargo run --bin hyperinfer-server
    ```

---

## 3. Authentication

The server uses **JWT-based authentication**. Tokens are passed in the `Authorization: Bearer <token>` header.

### 3.1. Logging In

```bash
curl -X POST http://localhost:8080/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email": "admin@hyperinfer.dev", "password": "admin"}'
```

The response includes a JWT token that you use for all subsequent admin requests.

### 3.2. Authenticated Requests

```bash
# Get the current user
curl http://localhost:8080/v1/auth/me \
  -H "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."

# Change password
curl -X POST http://localhost:8080/v1/auth/change-password \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"old_password": "...", "new_password": "..."}'

# Logout
curl -X POST http://localhost:8080/v1/auth/logout \
  -H "Authorization: Bearer $TOKEN"
```

### 3.3. Other Auth Endpoints

| Endpoint | Method | Purpose |
| :--- | :--- | :--- |
| `/v1/auth/login` | `POST` | Exchange email/password for a JWT. |
| `/v1/auth/me` | `GET` | Get info about the currently authenticated user. |
| `/v1/auth/logout` | `POST` | Invalidate the current session. |
| `/v1/auth/change-password` | `POST` | Change the current user's password. |

---

## 4. REST API Overview

The control plane exposes a REST API for all admin operations. All endpoints (except `/v1/auth/login`) require a valid JWT in the `Authorization` header.

### 4.1. Resource Endpoints

| Resource | Endpoints |
| :--- | :--- |
| **Teams** | `GET/POST /v1/teams`, `GET /v1/teams/{id}` |
| **Users** | `GET/POST /v1/users`, `GET /v1/users/{id}` |
| **API Keys** | `GET/POST /v1/api_keys`, `GET /v1/api_keys/{id}`, `POST /v1/api_keys/{id}/revoke` |
| **Model Aliases** | `GET/POST /v1/model_aliases`, `GET /v1/model_aliases/{id}` |
| **Quotas** | `GET/POST /v1/quotas`, `GET /v1/quotas/{team_id}` |
| **Deployments** | `GET/POST /v1/deployments`, `GET/PATCH/DELETE /v1/deployments/{id}` |
| **Routing** | `GET/PUT /v1/routing/config`, `GET /v1/routing/health` |
| **Config Sync** | `GET /v1/config/sync` |

### 4.2. Chat Completions

The control plane also proxies chat requests through the routing engine:

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

---

## 5. Managing Deployments

Deployments represent a specific LLM endpoint that the data plane can route to. See the [Providers Guide](providers.md) for a full deep-dive.

### 5.1. Example: Create a Deployment

```bash
curl -X POST http://localhost:8080/v1/deployments \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "gpt-4o-mini-prod",
    "provider": "openai",
    "model": "gpt-4o-mini",
    "base_url": "https://api.openai.com/v1",
    "api_key_ref": "openai_prod",
    "weight": 1
  }'
```

See [providers.md](providers.md#23-deployment-fields) for the full list of accepted fields.

---

## 6. Health Checks

The control plane exposes a basic liveness endpoint:

```bash
curl http://localhost:8080/healthz
# {"status":"ok"}
```

!!! note "Limited Health Endpoints"
    Currently, only the `/healthz` liveness endpoint is exposed. A `/readyz` readiness probe and a `/metrics` Prometheus endpoint are planned follow-ups.

---

## 7. Comparison Table: Control Plane vs. Data Plane

| Feature | Control Plane (Server) | Data Plane (Client) |
| :--- | :--- | :--- |
| **Primary Role** | Manage config, auth, and routing | Forward LLM requests |
| **Latency Sensitivity** | Low (admin operations) | Critical (microseconds matter) |
| **State** | Stateful (owns the database) | Stateless (reads from Redis) |
| **Scaling** | Vertical (usually 1-3 replicas) | Horizontal (add more instances) |
| **User-Facing** | No (admin-only) | Yes (your applications) |
| **Requires DB** | Yes (PostgreSQL) | No (only Redis) |
