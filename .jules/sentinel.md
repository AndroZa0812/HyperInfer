## 2024-05-18 - Prevented permissive CORS fallback
**Vulnerability:** Insecure CORS initialization fallback. The application defaulted to permitting `http://localhost:3000` via a hardcoded default origin when the `ALLOWED_ORIGINS` environment variable was unparseable or empty.
**Learning:** Hardcoding insecure fallbacks for missing environment variables can lead to production systems silently adopting insecure configurations rather than failing fast and forcing administrators to provide valid parameters.
**Prevention:** Remove fallback logic and throw errors when sensitive configuration values (like allowed CORS origins) are invalid or not provided, following fail-secure principles.
