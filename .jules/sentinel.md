## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## $(date +%Y-%m-%d) - SSRF via DNS Rebinding Bypass
**Vulnerability:** The application was vulnerable to SSRF (Server-Side Request Forgery) because it only checked the raw URL string against blocked IPs. An attacker could supply a domain name (like `127.0.0.1.nip.io`) that bypassed the string check but resolved to an internal IP when the HTTP client actually fetched it. Furthermore, it was vulnerable to DNS Rebinding where the initial check would see a safe IP but a subsequent request would connect to an internal IP.
**Learning:** `reqwest` exposes a `Resolve` trait which intercepts all DNS lookups at the socket connection phase. This guarantees validation at the Time-Of-Use, natively preventing both generic domain bypasses and DNS Rebinding.
**Prevention:** Always implement DNS validation using custom resolvers attached directly to the HTTP client (e.g. `reqwest::dns::Resolve`), rather than checking strings or doing out-of-band resolution before the HTTP request.
