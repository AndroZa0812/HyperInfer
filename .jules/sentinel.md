## 2025-05-16 - [Enforce Strict JWT Expiration]
**Vulnerability:** The system allowed bypass of the `"exp"` (expiration) claim verification in JWTs via an `allow_insecure_exp` flag, and defined the claim as optional (`Option<u64>`) in the claims struct.
**Learning:** Even if intended for development or internal debugging, keeping mechanisms that selectively disable critical security checks (like token expiration) in the codebase introduces a risk that they might be enabled in production.
**Prevention:** Remove optionality and bypass flags for security-critical validation logic (e.g., JWT expiration). Mandatory claims should be explicitly defined as non-optional types in Rust structures, and default safe validation settings should be strictly enforced.
