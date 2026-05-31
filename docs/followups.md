# Follow-up Tasks

Issues and improvements discovered during docker-compose setup and deployment.

## Issues Found

### 1. Docker not installed - using Podman instead
- **Severity:** Low (workaround available)
- **Description:** System has Podman installed instead of Docker. `podman-compose` works as a drop-in replacement.
- **Workaround:** Use `podman-compose` instead of `docker compose`
- **Action:** Consider adding podman-compose to documentation or providing Docker installation instructions

### 2. Podman registry configuration missing
- **Severity:** Medium
- **Description:** Podman requires explicit registry configuration for unqualified image names. Without it, image pulls fail with "short-name did not resolve to an alias" error.
- **Workaround:** Created `~/.config/containers/registries.conf` with:
  ```toml
  unqualified-search-registries = ["docker.io", "quay.io"]
  ```
- **Action:** Add registry configuration instructions to README or provide a setup script

### 3. ~~Podman-compose variable substitution bug~~ [RESOLVED]
- **Severity:** High
- **Description:** `podman-compose` (Python package) incorrectly handles nested variable substitution in docker-compose.yml. The syntax `${DATABASE_URL:-postgres://${POSTGRES_USER:-postgres}:...}` causes URL mangling.
- **Symptoms:** 
  - First attempt: `password authentication failed for user "${POSTGRES_USER"`
  - Second attempt: `database "hyperinfer:postgres@postgres:5432/hyperinfer}" does not exist`
- **Resolution:** Switched to Podman's native compose wrapper using docker-compose-v2:
  1. Installed docker-compose binary to `~/.docker/cli-plugins/docker-compose`
  2. Added `export PODMAN_COMPOSE_PROVIDER=~/.docker/cli-plugins/docker-compose` to `~/.zshrc`
  3. Use `podman compose` (without hyphen) instead of `podman-compose`
  4. Created `.dockerignore` to exclude `target/` and other large directories from build context
- **Result:** Full Compose spec interpolation now works correctly, including nested variable substitution like `${DATABASE_URL:-postgres://${POSTGRES_USER:-postgres}:...}`

### 4. Telemetry consumer timeout warnings
- **Severity:** Low (non-blocking)
- **Description:** Server logs show repeated "Telemetry consumer error: timed out. Reconnecting..." messages with exponential backoff
- **Root Cause:** Redis XREADGROUP on empty stream with short timeout
- **Impact:** No functional impact, just noisy logs
- **Action:** Consider increasing Redis stream read timeout or adjusting log level for this specific error

### 5. Missing .dockerignore file
- **Severity:** Medium
- **Description:** Without a `.dockerignore` file, the build context included the entire `target/` directory (1GB+), causing extremely slow builds
- **Resolution:** Created `.dockerignore` to exclude:
  - `target/` - Rust build artifacts
  - `.worktrees/` - Git worktrees
  - `.git/`, `.github/` - Git metadata
  - `.opencode/`, `.understand-anything/`, `.trunk/`, `.jules/`, `.changeset/` - Tool configs
  - `node_modules/` - Node dependencies
  - `.env*` files (except `.env.example`)
- **Result:** Build context reduced from 1GB+ to ~220KB

## Recommendations

### Documentation Updates
1. Add Podman setup instructions alongside Docker (including `PODMAN_COMPOSE_PROVIDER` configuration)
2. Document required registry configuration for Podman (`~/.config/containers/registries.conf`)
3. Document `.dockerignore` requirement for efficient builds
4. Add troubleshooting section for common issues

### Configuration Improvements
1. Provide a setup script that:
   - Checks for Docker/Podman availability
   - Configures Podman registries if needed
   - Installs docker-compose binary and sets `PODMAN_COMPOSE_PROVIDER`
   - Generates secrets automatically
   - Validates .env before starting services

2. Add health checks for the server service to enable proper dependency management

### Code Improvements
1. Adjust telemetry consumer timeout to reduce log noise
2. Add better error handling for empty Redis streams
3. Consider making telemetry consumer optional or configurable

## Environment Setup Summary

Working configuration achieved with:
- Podman 4.x + docker-compose-v2 (via `PODMAN_COMPOSE_PROVIDER`)
- User-level registries.conf for image resolution
- `.dockerignore` to exclude build artifacts from context
- Full Compose spec variable substitution support

### Setup Commands
```bash
# Install docker-compose binary
mkdir -p ~/.docker/cli-plugins
curl -SL "https://github.com/docker/compose/releases/latest/download/docker-compose-linux-x86_64" \
  -o ~/.docker/cli-plugins/docker-compose
chmod +x ~/.docker/cli-plugins/docker-compose

# Configure Podman to use docker-compose
echo 'export PODMAN_COMPOSE_PROVIDER=~/.docker/cli-plugins/docker-compose' >> ~/.zshrc
source ~/.zshrc

# Run services
podman compose up -d
```

## Test Credentials (Development Only)

- **Admin Email:** admin@hyperinfer.local
- **Admin Password:** changeme123
- **Admin Token:** (generated in .env)
- **Server URL:** http://localhost:3000

**Note:** Change these credentials before any production use.
