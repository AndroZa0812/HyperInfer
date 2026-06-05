# Rust Crate Publishing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add automated Rust crate publishing to crates.io via `cargo-workspaces` in the existing release workflow.

**Architecture:** Extend the existing `release.yml` workflow with a `publish-crates` job that runs after Python publishing. Use `cargo-workspaces` to handle version bumping and publishing in dependency order. Add `version` fields to `path` dependencies so crates.io can resolve them.

**Tech Stack:** Rust, cargo-workspaces, GitHub Actions, crates.io

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add `[workspace.dependencies]` with version fields |
| `crates/hyperinfer-core/Cargo.toml` | Modify | Add crates.io metadata |
| `crates/hyperinfer-providers/Cargo.toml` | Modify | Add metadata + version to path dep |
| `crates/hyperinfer-router/Cargo.toml` | Modify | Add metadata + version to path dep |
| `crates/hyperinfer-client/Cargo.toml` | Modify | Add metadata + version to path deps |
| `crates/hyperinfer-server/Cargo.toml` | Modify | Add metadata + version to path deps |
| `.github/workflows/release.yml` | Modify | Add `publish-crates` job + `rust-bump` input |

---

### Task 1: Add metadata to hyperinfer-core

**Files:**
- Modify: `crates/hyperinfer-core/Cargo.toml:1-6`

- [ ] **Step 1: Add crates.io metadata to hyperinfer-core**

Edit `crates/hyperinfer-core/Cargo.toml`, replace the `[package]` section:

```toml
[package]
name = "hyperinfer-core"
version = "0.1.1"
edition = "2021"
description = "Core types and traits for HyperInfer LLM Gateway"
license = "MIT"
repository = "https://github.com/AndroZa0812/HyperInfer"
homepage = "https://github.com/AndroZa0812/HyperInfer"
documentation = "https://docs.rs/hyperinfer-core"
readme = "README.md"
keywords = ["llm", "gateway", "ai"]
categories = ["api-bindings", "web-programming"]
```

- [ ] **Step 2: Verify the crate compiles**

Run: `cargo check -p hyperinfer-core`
Expected: No errors

- [ ] **Step 3: Dry-run publish to verify metadata**

Run: `cargo publish --dry-run --manifest-path crates/hyperinfer-core/Cargo.toml --allow-dirty`
Expected: Success (may warn about missing README.md, that's OK)

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-core/Cargo.toml
git commit -m "chore: add crates.io metadata to hyperinfer-core"
```

---

### Task 2: Add metadata to hyperinfer-providers

**Files:**
- Modify: `crates/hyperinfer-providers/Cargo.toml:1-6`

- [ ] **Step 1: Add crates.io metadata to hyperinfer-providers**

Edit `crates/hyperinfer-providers/Cargo.toml`, replace the `[package]` section:

```toml
[package]
name = "hyperinfer-providers"
version = "0.1.0"
edition = "2021"
description = "Modular LLM provider system for HyperInfer"
license = "MIT"
repository = "https://github.com/AndroZa0812/HyperInfer"
homepage = "https://github.com/AndroZa0812/HyperInfer"
documentation = "https://docs.rs/hyperinfer-providers"
readme = "README.md"
keywords = ["llm", "gateway", "ai", "openai", "anthropic"]
categories = ["api-bindings", "web-programming"]
```

- [ ] **Step 2: Add version to path dependency**

In the same file, update the `hyperinfer-core` dependency:

```toml
hyperinfer-core = { path = "../hyperinfer-core", version = "0.1.1" }
```

- [ ] **Step 3: Verify the crate compiles**

Run: `cargo check -p hyperinfer-providers`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-providers/Cargo.toml
git commit -m "chore: add crates.io metadata to hyperinfer-providers"
```

---

### Task 3: Add metadata to hyperinfer-router

**Files:**
- Modify: `crates/hyperinfer-router/Cargo.toml:1-7`

- [ ] **Step 1: Add crates.io metadata to hyperinfer-router**

Edit `crates/hyperinfer-router/Cargo.toml`, replace the `[package]` section:

```toml
[package]
name = "hyperinfer-router"
version = "0.1.0"
edition = "2021"
description = "Intelligent request routing engine for HyperInfer"
license = "MIT"
repository = "https://github.com/AndroZa0812/HyperInfer"
homepage = "https://github.com/AndroZa0812/HyperInfer"
documentation = "https://docs.rs/hyperinfer-router"
readme = "README.md"
keywords = ["llm", "gateway", "routing", "load-balancing"]
categories = ["api-bindings", "web-programming"]
```

- [ ] **Step 2: Add version to path dependency**

In the same file, update the `hyperinfer-core` dependency:

```toml
hyperinfer-core = { path = "../hyperinfer-core", version = "0.1.1" }
```

- [ ] **Step 3: Verify the crate compiles**

Run: `cargo check -p hyperinfer-router`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-router/Cargo.toml
git commit -m "chore: add crates.io metadata to hyperinfer-router"
```

---

### Task 4: Add metadata to hyperinfer-client

**Files:**
- Modify: `crates/hyperinfer-client/Cargo.toml:1-6`

- [ ] **Step 1: Add crates.io metadata to hyperinfer-client**

Edit `crates/hyperinfer-client/Cargo.toml`, replace the `[package]` section:

```toml
[package]
name = "hyperinfer-client"
version = "0.2.0"
edition = "2021"
description = "High-level client SDK for HyperInfer LLM Gateway"
license = "MIT"
repository = "https://github.com/AndroZa0812/HyperInfer"
homepage = "https://github.com/AndroZa0812/HyperInfer"
documentation = "https://docs.rs/hyperinfer-client"
readme = "README.md"
keywords = ["llm", "gateway", "client", "sdk"]
categories = ["api-bindings", "web-programming"]
```

- [ ] **Step 2: Add version to path dependencies**

In the same file, update the internal dependencies:

```toml
hyperinfer-core = { path = "../hyperinfer-core", version = "0.1.1" }
hyperinfer-router = { path = "../hyperinfer-router", version = "0.1.0" }
hyperinfer-providers = { path = "../hyperinfer-providers", version = "0.1.0", features = [
  "openai",
  "anthropic",
] }
```

- [ ] **Step 3: Verify the crate compiles**

Run: `cargo check -p hyperinfer-client`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add crates/hyperinfer-client/Cargo.toml
git commit -m "chore: add crates.io metadata to hyperinfer-client"
```

---

### Task 5: Add metadata to hyperinfer-server

**Files:**
- Modify: `crates/hyperinfer-server/Cargo.toml:1-6`

- [ ] **Step 1: Add crates.io metadata to hyperinfer-server**

Edit `crates/hyperinfer-server/Cargo.toml`, replace the `[package]` section:

```toml
[package]
name = "hyperinfer-server"
version = "0.1.1"
edition = "2021"
description = "High-performance LLM Gateway server built with Axum"
license = "MIT"
repository = "https://github.com/AndroZa0812/HyperInfer"
homepage = "https://github.com/AndroZa0812/HyperInfer"
documentation = "https://docs.rs/hyperinfer-server"
readme = "README.md"
keywords = ["llm", "gateway", "server", "axum", "api"]
categories = ["web-programming::http-server", "api-bindings"]
```

- [ ] **Step 2: Add version to path dependencies**

In the same file, update the internal dependencies:

```toml
hyperinfer-core = { path = "../hyperinfer-core", version = "0.1.1" }
hyperinfer-router = { path = "../hyperinfer-router", version = "0.1.0" }
```

Also update the dev-dependencies:

```toml
[dev-dependencies]
hyperinfer-core = { path = "../hyperinfer-core", version = "0.1.1", features = ["test-mocks"] }
```

- [ ] **Step 3: Create README.md**

Create `crates/hyperinfer-server/README.md`:

```markdown
# hyperinfer-server

High-performance LLM Gateway server built with Axum.

## Features

- Multi-provider LLM routing (OpenAI, Anthropic, Azure, etc.)
- Rate limiting and request queuing
- API key management and authentication
- PostgreSQL persistence
- Redis caching
- OpenTelemetry observability

## Usage

```rust
use hyperinfer_server::Server;

#[tokio::main]
async fn main() {
    let server = Server::new().await;
    server.run().await;
}
```

## License

MIT
```

- [ ] **Step 4: Verify the crate compiles**

Run: `cargo check -p hyperinfer-server`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add crates/hyperinfer-server/Cargo.toml crates/hyperinfer-server/README.md
git commit -m "chore: add crates.io metadata to hyperinfer-server"
```

---

### Task 6: Add workspace dependencies to root Cargo.toml

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add workspace dependencies section**

Edit `Cargo.toml`, append after the existing content:

```toml

[workspace.dependencies]
hyperinfer-core = { path = "crates/hyperinfer-core", version = "0.1.1" }
hyperinfer-providers = { path = "crates/hyperinfer-providers", version = "0.1.0" }
hyperinfer-router = { path = "crates/hyperinfer-router", version = "0.1.0" }
hyperinfer-client = { path = "crates/hyperinfer-client", version = "0.2.0" }
hyperinfer-server = { path = "crates/hyperinfer-server", version = "0.1.1" }
```

- [ ] **Step 2: Verify workspace resolves**

Run: `cargo metadata --format-version 1 --no-deps > /dev/null`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore: add workspace dependencies with version fields"
```

---

### Task 7: Add publish-crates job to release workflow

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Add rust-bump input to workflow_dispatch**

In `.github/workflows/release.yml`, add after the `bump` input (around line 21):

```yaml
      rust-bump:
        description: "Rust crate version bump (none to skip)"
        required: false
        type: choice
        options:
          - none
          - alpha
          - beta
          - rc
          - patch
          - minor
          - major
        default: none
```

- [ ] **Step 2: Add publish-crates job**

In `.github/workflows/release.yml`, add a new job after the `publish` job and before `github-release`:

```yaml
  # ---------------------------------------------------------------------------
  # Publish Rust crates to crates.io via cargo-workspaces
  # ---------------------------------------------------------------------------
  publish-crates:
    name: Publish Rust crates
    needs: publish
    if: github.event_name == 'workflow_dispatch' && inputs.rust-bump != 'none'
    runs-on: ubuntu-latest
    environment: crates-io
    steps:
      - uses: actions/checkout@v6
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Install cargo-workspaces
        run: cargo install cargo-workspaces --locked

      - name: Publish crates
        run: |
          if [ "${{ inputs.rust-bump }}" = "alpha" ] || [ "${{ inputs.rust-bump }}" = "beta" ] || [ "${{ inputs.rust-bump }}" = "rc" ]; then
            cargo ws publish --pre-id ${{ inputs.rust-bump }} --exact --yes --token ${{ secrets.CARGO_REGISTRY_TOKEN }} --allow-branch main
          else
            cargo ws publish ${{ inputs.rust-bump }} --exact --yes --token ${{ secrets.CARGO_REGISTRY_TOKEN }} --allow-branch main
          fi
```

- [ ] **Step 3: Update github-release job to depend on publish-crates**

Change the `github-release` job's `needs` from:

```yaml
    needs: publish
```

to:

```yaml
    needs: [publish, publish-crates]
    if: always() && needs.publish.result == 'success'
```

- [ ] **Step 4: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml')); print('YAML valid')"`
Expected: `YAML valid`

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "feat: add publish-crates job to release workflow"
```

---

### Task 8: Test dry-run locally

- [ ] **Step 1: Install cargo-workspaces locally**

Run: `cargo install cargo-workspaces --locked`
Expected: Installation completes successfully

- [ ] **Step 2: Dry-run version bump**

Run: `cargo ws publish --pre-id alpha --exact --yes --dry-run --allow-branch main`
Expected: Shows version bump and publish plan without making changes

- [ ] **Step 3: Verify all crates pass cargo check**

Run: `cargo check --workspace`
Expected: No errors

---

### Task 9: Push and trigger first release

- [ ] **Step 1: Push all changes**

```bash
git push origin main
```

- [ ] **Step 2: Add CARGO_REGISTRY_TOKEN to GitHub secrets**

Go to: https://github.com/AndroZa0812/HyperInfer/settings/secrets/actions
Add secret: `CARGO_REGISTRY_TOKEN` with your crates.io API token

- [ ] **Step 3: Create crates-io environment (optional)**

Go to: https://github.com/AndroZa0812/HyperInfer/settings/environments
Create environment: `crates-io` (add protection rules if desired)

- [ ] **Step 4: Trigger release workflow**

```bash
gh workflow run release.yml \
  -f packages="hyperinfer-python,hyperinfer-langchain,hyperinfer-llamaindex" \
  -f bump=patch \
  -f rust-bump=alpha
```

- [ ] **Step 5: Monitor the workflow**

```bash
gh run watch
```

Expected: All jobs complete successfully, crates appear on crates.io with alpha versions

---

## Verification Checklist

After implementation, verify:

- [ ] All 5 crates have complete metadata (description, license, repository, etc.)
- [ ] Path dependencies include `version` fields
- [ ] `cargo check --workspace` passes
- [ ] `cargo ws publish --dry-run` succeeds
- [ ] Workflow YAML is valid
- [ ] `publish-crates` job runs only when `rust-bump != none`
- [ ] `github-release` job waits for both Python and Rust publishing
