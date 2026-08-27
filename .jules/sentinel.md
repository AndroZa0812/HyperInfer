## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2024-XX-XX - Stop Exposing Raw SQL Errors
**Vulnerability:** Raw sqlx and redis errors were exposed in error enum strings, potentially leading to information disclosure.
**Learning:** Returning generic database errors instead of raw unique constraint or underlying framework logs prevents potential information leaks.
**Prevention:** Implement custom generic display messages for the DbError and ConfigError structs in `hyperinfer-core/src/error.rs`.
