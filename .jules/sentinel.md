## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2024-05-24 - [Information Leakage via Database Errors]
**Vulnerability:** The backend leaked sensitive internal database schema and constraint details by bubbling up raw `sqlx` unique constraint error messages (e.g. `Key (email)=(test@example.com) already exists.`) directly to clients via HTTP 500/409 responses, permitting enumeration and information disclosure.
**Learning:** Returning `?` directly on `sqlx::query_as(...).fetch_one(...)` without mapping `sqlx::Error` propagates database-specific errors that are eventually converted into string responses by the router, causing unintentional data leakage.
**Prevention:** Always map database query results using `.map_err()` to catch specific raw errors (like `e.as_database_error().map(|db| db.is_unique_violation())`) and translate them into safe, domain-specific application errors (like `DbError::UniqueViolation("Resource already exists".to_string())`) with static messages.
