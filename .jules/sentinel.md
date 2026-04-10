## 2026-04-10 - [Critical] Remove Insecure JWT Expiration Bypass
**Vulnerability:** The `mcp.rs` server codebase had an option (`allow_insecure_exp`) and a corresponding state initializer (`new_with_insecure_exp`) that bypassed JWT expiration (`exp`) validation.
**Learning:** Even if disabled by default, keeping bypass logic for critical security controls (like JWT expiration) in production code can lead to severe security risks if accidentally enabled or misused.
**Prevention:** Remove insecure bypass flags entirely from production code; expiration should always be strictly enforced for JWT tokens.
