## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2025-02-25 - DNS Rebinding SSRF Protection
**Vulnerability:** The `validate_base_url` function relied on synchronous, pre-flight parsing of the host IP address to block restricted (e.g., private, loopback) IPs. This is vulnerable to DNS Rebinding (TOCTOU), where an attacker could provide a domain name that initially resolves to a safe IP during validation but subsequently resolves to a restricted IP when `reqwest::Client` makes the actual HTTP request.
**Learning:** Pre-flight synchronous host validation is fundamentally insufficient for defending against DNS rebinding in HTTP clients. Security controls for network requests must be enforced post-resolution, at the time the connection is established.
**Prevention:** Always implement a custom asynchronous DNS resolver for the HTTP client (e.g., via `reqwest::dns::Resolve`). The custom resolver must ensure that all IPs returned by the DNS query are validated against restricted ranges (e.g., using `is_loopback()`, `is_private()`) before they are used to establish a connection. Ensure the HTTP client is initialized with a fail-closed approach if configuring the resolver fails.
