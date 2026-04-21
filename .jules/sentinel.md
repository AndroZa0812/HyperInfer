## 2024-05-15 - Unsalted Hash for API Key
**Vulnerability:** hash_key uses unsalted SHA-256 which allows pre-computation attacks.
**Learning:** When hashing API keys or passwords, a salt or proper KDF like bcrypt/argon2 should be used.
**Prevention:** Use a standard password hashing library for API key hashes.

## 2024-05-15 - Optional JWT Expiration
**Vulnerability:** McpClaims allows exp to be Option<u64>, meaning tokens could never expire if generated without an exp claim.
**Learning:** JWT tokens should have mandatory expiration to limit the window of compromise.
**Prevention:** Enforce exp to be mandatory (u64) and do not support allow_insecure_exp options.
