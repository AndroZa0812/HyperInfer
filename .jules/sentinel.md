## 2024-05-24 - [Enforce HTTPS for Langfuse Basic Auth Credentials]
**Vulnerability:** The `init_langfuse_telemetry` function accepted HTTP endpoints, which could allow Langfuse API credentials (public and secret keys encoded as Basic Auth) to be transmitted over unencrypted connections, exposing them to interception.
**Learning:** External integrations that use Basic Auth for telemetry or API access must strictly enforce HTTPS, even if the endpoint is configurable by the user. Allowing HTTP creates a risk of silent credential leakage in production or staging environments if misconfigured.
**Prevention:** Hardcode protocol validation logic to reject `http://` unless the target is a known local development address (e.g., `localhost` or `127.0.0.1`).
