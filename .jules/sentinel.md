## 2024-05-24 - Remove Insecure Hardcoded JWT Secret Fallback
**Vulnerability:** A hardcoded default `MCP_JWT_SECRET` ("hyperinfer-dev-secret") could be activated via an environment variable flag (`MCP_INSECURE_DEV_MODE=1`). This fallback risks being enabled maliciously or by accident in production environments, creating a severe vulnerability where an attacker could forge valid JWTs.
**Learning:** The fallback logic was historically kept for developer convenience but undermines security by leaving a path to enable a well-known weak secret.
**Prevention:** Strictly require authentication secrets to be provided via environment variables with no hardcoded fallbacks or insecure flags that compromise authentication validation.
