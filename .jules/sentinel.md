## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2026-07-13 - [SSRF Mitigation in HTTP Client]
**Vulnerability:** The HTTP client used for proxying requests (`reqwest::Client`) manually validated URLs to block private IPs but did not implement a custom DNS resolver, leaving it vulnerable to Server-Side Request Forgery (SSRF) via DNS rebinding.
**Learning:** Pre-flight URL validation based on the hostname's resolution is insufficient due to TOCTOU vulnerabilities (Time-Of-Check to Time-Of-Use). If a custom `reqwest::dns::Resolve` is not provided, the host could resolve to a benign IP during validation and a private IP during connection.
**Prevention:** Always implement a custom `reqwest::dns::Resolve` trait to asynchronously filter out private, loopback, link-local, broadcast, unspecified, and documentation IPs post-resolution before establishing the TCP connection.
