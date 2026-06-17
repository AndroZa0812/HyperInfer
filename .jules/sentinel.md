## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-27 - [Fix SSRF via DNS Rebinding in Proxy Requests]
**Vulnerability:** The application mitigated SSRF by resolving and verifying the hostname before making the actual request using `reqwest::Client`. This Time-of-Check to Time-of-Use (TOCTOU) approach is susceptible to DNS rebinding, where an attacker’s domain initially resolves to a safe IP but changes to a blocked, internal IP when `reqwest` performs its own DNS resolution.
**Learning:** Validating IPs before connection doesn't guarantee the connection will use those IPs due to DNS caching and TTL manipulations.
**Prevention:** Always implement DNS validation within the HTTP client's connection phase. By using a custom `reqwest::dns::Resolve` implementation (`SafeResolver`), we verify the actually resolved IP at the time of connection and enforce blocked IP restrictions reliably.
