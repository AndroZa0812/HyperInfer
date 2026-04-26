## 2025-03-02 - Strict JWT Expiration Enforcement
**Vulnerability:** JWT expiration (`exp` claim) enforcement could be optionally bypassed in the MCP authentication middleware via `allow_insecure_exp`.
**Learning:** Permitting insecure bypass modes for critical authentication checks (like JWT validation) in shared library or core service code can easily lead to accidental production deployments with weakened security, especially when toggled via flags.
**Prevention:** Never include code paths that intentionally downgrade authentication security (such as removing required claims during validation) in production services, even for developer convenience. Use dedicated test tokens or mock providers for local development instead.
