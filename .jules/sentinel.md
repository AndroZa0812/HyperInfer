## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2024-08-14 - Information Leakage via Database Errors in `db.rs`
**Vulnerability:** Raw `sqlx::Error` unique constraint violations in `create_user`, `create_api_key`, `create_model_alias`, and `create_quota` were propagating via the `?` operator directly into `DbError::Sqlx`, which caused them to bubble up as internal server errors (500) and leak sensitive database schema information or enable enumeration attacks.
**Learning:** Returning unmapped `sqlx` errors from database creation methods directly exposes application state, especially for constraints that correspond to duplicate items (e.g., account enumeration).
**Prevention:** Always catch and explicitly map `sqlx` unique violations using `e.as_database_error().map(|db| db.is_unique_violation())` to safe, domain-specific `DbError::UniqueViolation` variants with static, generic error messages.
