## 2024-08-29 - Prevent Information Exposure via Error Messages
**Vulnerability:** Information Exposure (CWE-209) in `crates/hyperinfer-core/src/error.rs`.
**Learning:** The `thiserror` crate's `#[error(...)]` attribute was formatting inner errors (like `sqlx::Error` and `redis::RedisError`) directly into the `Display` string. This could expose raw error messages, stack traces, internal paths, or SQL syntax to clients or log aggregators that serialize `Display` output.
**Prevention:** Ensure enum variants that wrap underlying infrastructure errors omit the `{0}` placeholder from their `#[error(...)]` definitions. This allows the internal application to use `std::error::Error::source()` or `Debug` (`{:?}`) for detailed logging while presenting a safe, generic string to external interfaces.
