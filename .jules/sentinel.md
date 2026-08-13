## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2024-08-13 - [Information Leakage via Unhandled DB Errors]
**Vulnerability:** Raw sqlx database errors were being returned directly to clients for unique constraint violations, potentially exposing sensitive database schema details, table names, and constraint logic via 500 Internal Server Errors.
**Learning:** Returning unhandled database exceptions to end users violates the secure failure principle (CWE-209). By default, ORMs/Database drivers include verbose debugging information in their error strings.
**Prevention:** Always map database-level errors (like `sqlx::Error`) to safe, domain-specific application errors (like `DbError::UniqueViolation`) with static, generic error messages before they cross the API boundary.
