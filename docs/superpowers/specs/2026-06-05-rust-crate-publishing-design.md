# Rust Crate Publishing Design

**Date**: 2026-06-05  
**Status**: Approved  
**Author**: HyperInfer Team

## Overview

This design adds automated Rust crate publishing to the existing release workflow. When triggered via `workflow_dispatch`, the workflow will publish all 4 library crates to crates.io using `cargo-workspaces`, with support for pre-release versions during the unstable phase.

## Goals

- Publish Rust crates to crates.io as part of the unified release flow
- Support pre-release versions (e.g., `0.1.0-alpha.1`) for unstable libraries
- Maintain independent versioning per crate
- Reuse existing `workflow_dispatch` trigger alongside Python releases
- Ensure crates publish in correct dependency order

## Non-Goals

- Publishing `hyperinfer-python` (already covered by PyPI release)
- Automatic version detection from git history
- Publishing on every push to main (manual dispatch only for now)

## Architecture

### Workflow Flow

```
prepare → build (Python) → publish (TestPyPI) → publish-crates (crates.io) → github-release
```

The `publish-crates` job:
1. Runs after Python `publish` succeeds
2. Installs `cargo-workspaces` CLI
3. Uses `cargo ws publish` to publish all crates in topological order
4. Requires `CARGO_REGISTRY_TOKEN` GitHub secret

### Dependency Graph

```
hyperinfer-core (no internal deps)
  ↓
hyperinfer-providers (depends on core)
hyperinfer-router (depends on core)
  ↓
hyperinfer-client (depends on core, providers, router)
hyperinfer-server (depends on core, router)
```

`cargo-workspaces` automatically resolves this and publishes in order.

## Detailed Changes

### 1. Workspace Root `Cargo.toml`

Add `version` fields to `path` dependencies so crates.io can resolve them:

```toml
[workspace.dependencies]
hyperinfer-core = { path = "crates/hyperinfer-core", version = "0.1.1" }
hyperinfer-providers = { path = "crates/hyperinfer-providers", version = "0.1.0" }
hyperinfer-router = { path = "crates/hyperinfer-router", version = "0.1.0" }
hyperinfer-client = { path = "crates/hyperinfer-client", version = "0.2.0" }
hyperinfer-server = { path = "crates/hyperinfer-server", version = "0.1.1" }
```

### 2. Crate `Cargo.toml` Metadata

Each publishable crate needs:

```toml
[package]
name = "hyperinfer-core"
version = "0.1.1"
description = "Core types and traits for HyperInfer LLM Gateway"
license = "MIT"
repository = "https://github.com/AndroZa0812/HyperInfer"
homepage = "https://github.com/AndroZa0812/HyperInfer"
documentation = "https://docs.rs/hyperinfer-core"
readme = "README.md"
keywords = ["llm", "gateway", "ai"]
categories = ["api-bindings", "web-programming"]
```

### 3. Workflow Changes

#### New `publish-crates` Job

```yaml
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

    - name: Bump versions
      run: |
        if [ "${{ inputs.rust-bump }}" = "alpha" ] || [ "${{ inputs.rust-bump }}" = "beta" ] || [ "${{ inputs.rust-bump }}" = "rc" ]; then
          cargo ws version --preid ${{ inputs.rust-bump }} --exact --yes
        else
          cargo ws version ${{ inputs.rust-bump }} --exact --yes
        fi

    - name: Publish crates
      run: cargo ws publish --from-git --token ${{ secrets.CARGO_REGISTRY_TOKEN }} --allow-branch main
```

#### New `workflow_dispatch` Input

Add `rust-bump` alongside existing `bump`:

```yaml
inputs:
  rust-bump:
    description: "Rust crate version bump type"
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

#### Version Bumping Logic

The `Bump versions` step above handles both pre-release and stable bumps:
- Pre-release (`alpha`, `beta`, `rc`): generates `X.Y.Z-alpha.N`
- Stable (`patch`, `minor`, `major`): generates standard semver

`cargo ws version` automatically:
- Bumps all crates in dependency order
- Updates internal dependency versions
- Commits changes with message "chore: release"

### 4. Pre-Release Strategy

**During unstable phase:**
- Use `cargo ws version --preid alpha` to generate `X.Y.Z-alpha.N`
- Example: `0.1.1` → `0.1.2-alpha.1` → `0.1.2-alpha.2` → `0.1.2`

**After stabilization:**
- Switch to standard semver: `patch`, `minor`, `major`
- Pre-release versions are only resolved when explicitly requested

### 5. Security

- `CARGO_REGISTRY_TOKEN` stored as GitHub secret (scoped to `crates-io` environment)
- Token has publish-only permissions (no admin rights)
- Consider using OIDC trusted publishing in the future (crates.io supports it)

## Testing and Verification

### Local Testing

```bash
# Dry-run publish
cargo ws publish --dry-run

# Verify metadata
cargo publish --dry-run --manifest-path crates/hyperinfer-core/Cargo.toml
```

### CI Verification

- Add `cargo ws publish --dry-run` to the `detect` job on push events
- Fail fast if metadata is incomplete or dependencies are unresolvable

### Rollout Plan

1. Add `CARGO_REGISTRY_TOKEN` to GitHub secrets
2. Create `crates-io` environment (optional approval gate)
3. Run first dispatch with `rust-bump: alpha`
4. Verify crates appear on crates.io with pre-release versions
5. Test `cargo add hyperinfer-core@=0.1.2-alpha.1` from a test project

## Migration Path

**Current state:**
- Crates use `path` dependencies only
- No crates.io metadata

**Target state:**
- Crates publishable to crates.io
- Unified release flow for Python and Rust

**Steps:**
1. Add missing metadata to all 4 crate `Cargo.toml` files
2. Add `version` to workspace dependencies
3. Add `publish-crates` job to workflow
4. Add `rust-bump` input to `workflow_dispatch`
5. Test with `--dry-run` on a PR
6. Merge and trigger first real publish

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Publishing broken crates | Use `--dry-run` in CI, pre-release versions allow iteration |
| Version conflicts | `cargo-workspaces` handles dependency resolution |
| Token leak | Scoped secret, environment protection, audit logs |
| Dependency order wrong | `cargo-workspaces` uses topological sort |

## Future Improvements

- **OIDC trusted publishing**: Switch from token to OIDC when crates.io supports it
- **Changelog generation**: Use `git-cliff` or `cargo-release` for automated changelogs
- **MSRV policy**: Add `rust-version` field to enforce minimum Rust version
- **Feature flags**: Document which features are stable vs experimental
