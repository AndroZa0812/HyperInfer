## 2024-06-25 - [Removed Insecure JWT Expiry Bypass]
**Vulnerability:** The MCP state allowed an `allow_insecure_exp` flag to disable JWT expiry validation for development purposes, but it was being used in authentication logic, creating a risk where expired JWTs could bypass authentication.
**Learning:** Hardcoded dev mode flags for security checks can be forgotten or mistakenly used in production paths. Expiry validation must be enforced at the type level.
**Prevention:** Remove `allow_insecure_exp` flags entirely and ensure `exp` is a required claim in the `McpClaims` struct, failing validation if absent.
