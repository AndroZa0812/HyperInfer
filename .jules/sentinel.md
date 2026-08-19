## 2024-08-20 - Prevent Database Error Information Leakage
**Vulnerability:** Internal database schema details or SQL syntax could be leaked to clients via HTTP 500 responses when `DbError::Sqlx` formatting included the raw `sqlx::Error`.
**Learning:** Returning specific error messages like `"Team name already exists"` on a `create_team` endpoint does not pose an enumeration risk because the endpoint inherently acts on a specific resource type. Attempting to mask these specific resource names degrades UX without providing security.
**Prevention:** Do not format raw database errors (e.g., `sqlx::Error`) in the `Display` implementation for HTTP-facing error enums.
## 2025-02-25 - Prevent Database Error Information Leakage
**Vulnerability:** Internal database schema details or SQL syntax could be leaked to clients via HTTP 500 responses when `DbError::Sqlx` formatting included the raw `sqlx::Error`.
**Learning:** Returning specific error messages like `"Team name already exists"` on a `create_team` endpoint does not pose an enumeration risk because the endpoint inherently acts on a specific resource type. Attempting to mask these specific resource names degrades UX without providing security.
**Prevention:** Do not format raw database errors (e.g., `sqlx::Error`) in the `Display` implementation for HTTP-facing error enums.
