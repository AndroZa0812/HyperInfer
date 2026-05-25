## 2025-05-18 - Enforce HTTPS for Langfuse Telemetry
**Vulnerability:** The `init_langfuse_telemetry` function accepted arbitrary HTTP URLs, allowing Langfuse Basic Auth credentials (derived from public and secret keys) to be sent over plaintext connections to remote hosts.
**Learning:** Hardcoded telemetry initialization lacking explicit transport security checks can inadvertently expose credentials during debugging or misconfiguration, bypassing typical environment validation.
**Prevention:** Implement explicit connection validation (e.g., enforcing HTTPS or restricting to localhost) prior to encoding and transmitting credentials over network requests.
