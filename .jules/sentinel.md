## 2024-04-07 - Avoid Mutable Globals in Tests
**Vulnerability:** Mutating `std::env::var` concurrently in async Rust tests causes severe data races and undefinable behavior on CI servers.
**Learning:** Configurations like `ADMIN_TOKEN` should be extracted via axum `State` instead of fetched directly from `std::env` inside the middleware, thereby allowing tests to mock the token securely in parallel without data races.
**Prevention:** Always use `State` injection in axum handlers and middlewares (`middleware::from_fn_with_state`) rather than reading directly from environment globals.
