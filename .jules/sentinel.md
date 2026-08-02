## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-14 - Prevent Raw SQL Error Leakage
**Vulnerability:** Core database creation operations (e.g., `create_user`, `create_api_key`) returned raw `sqlx::Error` on unique constraint violations instead of safely mapping them. This can leak database schema structure and echo untrusted user input back via generic 500 error responses, aiding error-based SQLi reconnaissance or triggering Reflected XSS.
**Learning:** Returning `?` on database `execute` or `fetch_one` calls propagates the underlying database engine error message.
**Prevention:** Always map database errors using `e.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false)` (or similar methods) to safely return standard domain errors like `DbError::UniqueViolation` with a static string rather than the raw database message.
## 2025-02-14 - Use Correct method for Unique Violation Check in SQLx
**Vulnerability:** Although mapping errors securely to `DbError::UniqueViolation` prevents database schema leakage, using a non-existent method `is_unique_violation()` on `sqlx::error::DatabaseError` causes compilation failures, disrupting CI.
**Learning:** `sqlx::error::DatabaseError` in modern `sqlx` (e.g. 0.7+) does not have an `is_unique_violation()` method. It requires matching on the error `kind()` like `db.kind() == sqlx::error::ErrorKind::UniqueViolation`.
**Prevention:** Verify APIs exist before using them in database mappings. Use `db.kind() == sqlx::error::ErrorKind::UniqueViolation` for reliable cross-engine constraint checks.
