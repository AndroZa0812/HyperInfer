## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2026-08-22 - SSRF Prevention via Custom DNS Resolver
**Vulnerability:** Server-Side Request Forgery (SSRF) bypasses (e.g., DNS Rebinding) on user-facing HTTP clients.
**Learning:** Simply validating hostnames synchronously before sending a request is insufficient because attackers can use DNS rebinding to resolve to a private IP after the check passes. However, applying a custom `SafeResolver` to all `reqwest::Client` instances globally breaks internal clients (like OpenTelemetry exporters) which must legitimately connect to local or internal IPs.
**Prevention:** Implement a custom `reqwest::dns::Resolve` trait (`SafeResolver`) that filters out private, loopback, and local network IPs *during* DNS resolution. Apply this secure resolver **only** to HTTP clients that handle user-supplied URLs (e.g., the proxy client), and leave internal system clients (e.g., telemetry exporters) using standard resolvers.
## 2026-08-22 - Fix manual_filter clippy lint in Python bindings
**Vulnerability:** None (Code quality/Linting)
**Learning:** Rust `clippy` will flag manual implementations of `Option::filter`, such as `.and_then(|v| if v.is_none() { None } else { Some(v) })`.
**Prevention:** Use the built-in `.filter(|v| !v.is_none())` or `.filter(|v| v.is_some())` to ensure clean, idiomatic code and avoid breaking CI jobs running with `-D warnings`.
