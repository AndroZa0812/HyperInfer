## 2024-05-27 - Information Disclosure via SQLx Errors
**Vulnerability:** The `Display` implementation for `DbError::Sqlx` in `hyperinfer-core/src/error.rs` formatted the raw `sqlx::Error`.
**Learning:** Returning raw database errors from `sqlx` in a generic error enum whose `Display` formatting is often logged or returned in API responses can leak internal database schemas, query logic, or stack traces to potential attackers.
**Prevention:** Always mask the `Display` string of wrapped third-party errors (like `sqlx::Error`) with a generic safe message (e.g., `"Database error"`). The `Debug` format or the source tree traversing methods (e.g. `error.source()`) should be used for internal server logs to maintain diagnosability without leaking information over the network.
