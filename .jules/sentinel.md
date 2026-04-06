## 2025-02-18 - Added Admin Authentication Middleware
**Vulnerability:** Missing authentication on the control plane `/v1/*` endpoints. Any client could create users, quotas, or retrieve api_keys and team objects.
**Learning:** The architecture implemented a nested axum router, however, authentication middleware (`jwt_auth_middleware`) was only applied to the `/mcp/*` paths, while `/v1/*` paths were mounted directly and accessible unconditionally. Hardcoded test logic was mixed in some scratch files.
**Prevention:** Group similar sensitive endpoints together using router nesting (e.g., `Router::new().nest("/v1", v1_router)`) and apply authentication middlewares explicitly at the layered router level, ensuring defense in depth across all administrative boundaries.
