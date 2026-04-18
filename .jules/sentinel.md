## 2024-04-18 - [Insecure JWT Expiration Bypass]
**Vulnerability:** The JWT validation logic contained an `allow_insecure_exp` flag which bypassed the `exp` (expiration) claim check when enabled. A token could bypass validation permanently without expiring.
**Learning:** Hardcoding or conditionally allowing insecure JWT validation is a major security flaw since it completely compromises the effectiveness of token expiration.
**Prevention:** Remove any code that conditionally allows bypassing expiration. Make the `exp` claim mandatory.
