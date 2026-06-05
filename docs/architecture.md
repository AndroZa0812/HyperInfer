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
