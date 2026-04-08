## Sentinel Journal
## 2024-11-20 - [CRITICAL] Remove Hardcoded Admin Secret & Insecure Dev Mode Bypass
**Vulnerability:** The server configuration for `ADMIN_TOKEN` included a fallback that bypassed authentication entirely if `MCP_INSECURE_DEV_MODE=1` was set, explicitly hardcoding the string `hyperinfer-admin-dev` as the fallback token.
**Learning:** Hardcoded dev mode fallbacks in production binaries carry a high risk of being accidentally or maliciously enabled in production environments, creating critical unauthorized access vectors to control-plane administrative endpoints.
**Prevention:** Never include hardcoded sensitive credentials or dev mode bypasses in server application code. The application should safely fail to start if required environment variables like `ADMIN_TOKEN` are not provided.
