## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2026-08-28 - [Mask Internal Database and Redis Error Details]
**Vulnerability:** The application was exposing underlying `sqlx` and `redis` error details in the `Display` implementation of `DbError::Sqlx` and `ConfigError::Redis`.
**Learning:** Returning raw underlying errors can leak internal database queries, schemas, or configurations to end users if these errors bubble up to HTTP responses.
**Prevention:** Avoid formatting the underlying errors via `{0}` in `thiserror`'s `#[error(...)]` attribute for low-level infrastructure errors like `sqlx::Error` or `redis::RedisError`.
