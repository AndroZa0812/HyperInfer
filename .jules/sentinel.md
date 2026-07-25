## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-26 - [Fix Leaking Database Details in Unique Constraint Violations]
**Vulnerability:** The application was catching SQLx unique constraint violations (like duplicate user emails or API keys) but instead of returning a specific, sanitized error, it wrapped them in `DbError::Sqlx(e)`, returning generic 500 Internal Server Errors that exposed internal database schema information or stack traces.
**Learning:** Unique constraints are a common form of business logic enforcement in the database layer. If these are not explicitly handled and translated into 409 Conflict application errors, we risk leaking internal state, confusing users, and creating poor error visibility.
**Prevention:** Always verify `e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false)` in the `Err` branch of `sqlx` queries that have `UNIQUE` constraints and map them to explicit `DbError::UniqueViolation` application variants.
