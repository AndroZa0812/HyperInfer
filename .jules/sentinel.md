## 2026-04-17 - [Enforce Strict JWT Expiration]
**Vulnerability:** The MCP JWT validation logic in `hyperinfer-server` allowed the `exp` claim to be optional and included a bypass mechanism (`allow_insecure_exp`) that could be used to skip expiration validation entirely.
**Learning:** Security bypasses designed for 'dev-only' environments can easily leak into production configurations or be misused, leading to critical vulnerabilities like indefinitely lived access tokens. JWT expiration must be strictly enforced at the structural level.
**Prevention:** Make security-critical fields (like `exp`) mandatory in payload structures (`Option<T>` -> `T`) and remove any configuration toggles that allow disabling essential security checks (like `allow_insecure_exp`).
