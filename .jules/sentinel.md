## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2024-07-06 - [SSRF Mitigation via DNS Rebinding Protection]
**Vulnerability:** The HTTP proxy (`crates/hyperinfer-server/src/proxy.rs`) was vulnerable to DNS rebinding attacks because it only validated the URL string against a blocklist of private IP prefixes before DNS resolution. An attacker could bypass this by using a malicious domain that resolves to an internal IP.
**Learning:** In Rust with `reqwest`, string-based pre-flight host validation is insufficient to prevent SSRF due to TOCTOU. We must implement a custom `reqwest::dns::Resolve` trait to filter IPs asynchronously post-resolution, while strictly relying on stable `std::net::IpAddr` methods like `is_private()` or `is_loopback()` rather than unstable ones like `is_documentation()`.
**Prevention:** Always use a custom DNS resolver that checks resolved IP addresses for internal/loopback ranges before the connection is established. Avoid using string-based prefix matching for IPs, as it can be bypassed by IPv4-mapped IPv6 addresses (e.g., `::ffff:127.0.0.1`).
