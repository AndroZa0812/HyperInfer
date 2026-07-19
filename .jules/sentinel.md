## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-27 - [Fix SSRF via DNS Rebinding in Proxy Client]
**Vulnerability:** The proxy client (`HTTP_CLIENT` in `proxy.rs`) performed synchronous host IP validation (`validate_base_url`) before making the request. This validation checked the resolved IP against a blocklist, but the actual request would resolve the hostname again, which is vulnerable to DNS Rebinding (TOCTOU).
**Learning:** Pre-flight synchronous host validation is insufficient to prevent SSRF because the IP address could change between validation and connection.
**Prevention:** Always implement a custom asynchronous DNS resolver (e.g., implementing `reqwest::dns::Resolve`) that filters out restricted IP addresses post-resolution and use it to configure the HTTP client.
