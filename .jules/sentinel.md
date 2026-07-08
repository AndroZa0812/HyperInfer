## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-26 - [Fix SSRF bypass via DNS Rebinding]
**Vulnerability:** The application mitigated Server-Side Request Forgery (SSRF) by validating hostnames synchronously before dispatching the request. This allows DNS Rebinding attacks where an attacker controls the DNS and responds with a safe IP first (bypassing validation) and then with a malicious internal IP when the actual request is made.
**Learning:** Pre-flight validation is insufficient against DNS Rebinding because the resolution can change between the check and the actual fetch (TOCTOU).
**Prevention:** Always use a custom DNS resolver attached to the HTTP client (e.g., `reqwest::dns::Resolve`) to filter resolved IP addresses at the time of connection. Ensure it fails closed if initialization fails.
