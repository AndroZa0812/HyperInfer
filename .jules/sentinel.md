## 2026-04-16 - [Enforce JWT Expiration]
**Vulnerability:** JWT tokens used in MCP could be created without an expiration date, and the `allow_insecure_exp` toggle allowed bypassing expiration checks entirely.
**Learning:** Incomplete JWT validation allows indefinitely valid tokens, which increases the attack surface for stolen tokens.
**Prevention:** Make the `exp` claim mandatory in `McpClaims` and remove all logic that permits bypassing token expiration validation.
