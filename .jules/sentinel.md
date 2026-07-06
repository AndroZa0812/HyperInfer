## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.

## 2024-07-06 - [SSRF Protection Bypass via DNS Rebinding]
**Vulnerability:** Pre-flight IP validation in `proxy.rs` (using string prefixes for blocked IPs on parsed URLs) was insufficient against DNS rebinding. `reqwest` could resolve the host to a private IP asynchronously after the pre-flight check passed.
**Learning:** When preventing SSRF in Rust using `reqwest`, synchronous pre-flight host validation is insufficient due to TOCTOU (DNS Rebinding) vulnerabilities. A custom `reqwest::dns::Resolve` trait is required to filter IPs asynchronously post-resolution.
**Prevention:** Always implement a custom `reqwest::dns::Resolve` trait (like `SafeResolver`) to filter IPs asynchronously post-resolution. Use native Rust IP checks (`is_private`, `is_loopback`) instead of error-prone string prefix matching. Ensure `reqwest::Client` builder uses `.expect()` for fail-closed initialization.
