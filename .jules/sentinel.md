## 2025-02-24 - [Fix Reflected User Input in InvalidUuid Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the invalid UUID string) directly in the `400 Bad Request` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `InvalidUuid(String)`. Use static error messages like `InvalidUuid` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-25 - [Fix Reflected User Input in Unique Violation Error]
**Vulnerability:** The application was reflecting untrusted, unsanitized user input (the team name) directly in the `409 Conflict` HTTP error response.
**Learning:** Returning unvalidated input directly in error messages can lead to Reflected XSS (if rendered by a client) or log forging. Rust's `thiserror` makes it easy to format strings, but we must be careful what strings we are formatting.
**Prevention:** Avoid allocating and reflecting raw user input in error enum variants like `UniqueViolation(String)`. Use static error messages like `UniqueViolation` for malformed input unless specific, sanitized context is required and safe to expose.
## 2025-02-14 - Prevent SSRF via DNS Rebinding in reqwest
**Vulnerability:** The proxy codebase verified base URLs by just parsing their hostnames into `IpAddr` and comparing against blocked private/loopback IPs. This permitted SSRF attacks where a public DNS record (like `127.0.0.1.nip.io`) could bypass the check, as it doesn't parse to an `IpAddr` directly but resolves to a blocked IP upon making the HTTP request, also known as a DNS rebinding attack.
**Learning:** Checking a hostname syntactically is insufficient for preventing SSRF. The domain must be resolved first, and each resolved IP checked.
**Prevention:** Instead of manual URL parsing and checking, implemented a custom `reqwest::dns::Resolve` struct (`SafeResolver`) on the `reqwest::Client`. This dynamically resolves the hostname before connecting and aborts the request securely if any of the underlying IPs are blocked, neutralizing DNS rebinding and domain-based SSRF exploits.
