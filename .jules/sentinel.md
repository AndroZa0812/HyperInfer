## 2025-02-14 - Removed insecure JWT expiration bypass in hyperinfer-server
**Vulnerability:** The MCP server component possessed a toggle (`allow_insecure_exp`) and associated construction methods (`McpState::new_with_insecure_exp`) that bypassed validation of the `exp` claim, allowing for immortal, never-expiring access tokens. Also, the `McpClaims` struct defined `exp` as an optional field.
**Learning:** Hardcoded overrides and optional expiration timestamps are often left enabled by accident, undermining the principle of least privilege and increasing the risk of leaked tokens being reused indefinitely.
**Prevention:** Always make the `exp` claim a strictly required (`u64`) property and remove conditional code paths that bypass verification logic completely in production code.
