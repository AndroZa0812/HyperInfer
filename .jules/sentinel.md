## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-26 - [Fix SSRF via DNS in Base URL Validation]
**Vulnerability:** The `validate_base_url` function checked hostnames against a hardcoded list of private IP prefixes via string matching. This allowed an attacker to bypass the filter using domains that resolve to private IPs (e.g., `127.0.0.1.nip.io`) or by using IPv6 loopback (`[::1]`).
**Learning:** Checking a hostname string against a list of private IP prefixes is insufficient because DNS resolution can mask the underlying private IP. We must resolve the hostname to its IP addresses and check those instead.
**Prevention:** Use `std::net::ToSocketAddrs` to resolve the parsed URL's hostname and validate the resolved `IpAddr` instances against loopback, unspecified, multicast, and RFC 1918 / RFC 4193 private ranges.
