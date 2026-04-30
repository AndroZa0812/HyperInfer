## 2025-02-28 - [Strict JWT Expiration Enforcement]
**Vulnerability:** The MCP server implementation contained an insecure developer override (`allow_insecure_exp`) that bypassed JWT token expiration validation and made the `exp` claim optional (`Option<u64>`).
**Learning:** Development convenience overrides for authentication can easily leak into production environments if they are deeply integrated into token validation logic rather than being handled gracefully.
**Prevention:** Always strictly enforce session expiration boundaries. Token specifications (like `exp`) should be defined as mandatory types (`u64` instead of `Option<u64>`) to fail safely at deserialization time, preventing bypass flags from overriding security constraints.
