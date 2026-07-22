## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2024-05-18 - [Fix SSRF via DNS Rebinding in Proxy]
**Vulnerability:** A critical SSRF vulnerability existed where users could bypass domain-based IP blocking using DNS rebinding, URL obfuscation, or 302 redirects, because IP validation was done before DNS resolution (TOCTOU) instead of safely at the connection phase.
**Learning:** Checking host strings in `validate_base_url` prior to resolution does not protect against DNS rebinding. While `reqwest`'s custom DNS resolver prevents most bypasses, it allows HTTP redirects by default which can loopback locally. Further, literal IPs skip the DNS resolver altogether in `reqwest/hyper`, and IPv4-mapped IPv6 addresses bypass simple subnet checks in many networking stacks.
**Prevention:** Implement a custom `reqwest::dns::Resolve` to enforce safe IP checks during connection, disable HTTP redirects via `.redirect(reqwest::redirect::Policy::none())`, maintain literal IP checks synchronously for `hyper` bypasses, and explicitly block IPv4-mapped IPv6 addresses, Unique Local Addresses, and Link-Local Addresses using recursive and bitwise validation checks.
