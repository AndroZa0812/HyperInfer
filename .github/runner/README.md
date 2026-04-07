# GitHub Actions Runner - Docker Setup

This directory contains a Docker-based GitHub Actions runner for running PR Agent with a local Gemma 4 model.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Your Machine                            │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │           Docker Container: github-runner                │   │
│  │  ┌──────────────────────────────────────────────────┐  │   │
│  │  │         GitHub Actions Runner                    │  │   │
│  │  │  - Receives GitHub webhooks                      │  │   │
│  │  │  - Runs PR Agent Docker containers               │  │   │
│  │  │  - Calls llama-server for inference              │  │   │
│  │  └──────────────────────────────────────────────────┘  │   │
│  │                         │                              │   │
│  │                         ▼                              │   │
│  │           network_mode: host                           │   │
│  └─────────────────────────┬──────────────────────────────┘   │
│                            │                                    │
│                            ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │  llama-server (on host or in separate container)        │   │
│  │  - Port 8001                                            │   │
│  │  - Gemma 4 model                                        │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Quick Start (Recommended: Host Network Mode)

This is the simplest setup and works best on Linux.

### 1. Build the Runner Image

```bash
cd .github/runner
docker-compose build
```

### 2. Get GitHub Runner Token

1. Go to: https://github.com/YOUR_USERNAME/HyperInfer/settings/actions/runners
2. Click "New self-hosted runner"
3. Select "Linux" and "x64"
4. Copy the token from the config command (looks like `ABCD...EFGH`)

### 3. Start llama-server with Gemma 4

```bash
# Option A: Using llama.cpp directly
./llama-server \
    -hf unsloth/gemma-4-26B-A4B-it-GGUF:UD-Q4_K_XL \
    --port 8001 \
    --temp 1.0 --top-p 0.95 --top-k 64 \
    --chat-template-kwargs '{"enable_thinking":true}'

# Option B: Using Unsloth Studio
curl -fsSL https://unsloth.ai/install.sh | sh
unsloth studio -H 0.0.0.0 -p 8001
```

### 4. Start the GitHub Runner

```bash
cd .github/runner
export REPO_URL=https://github.com/YOUR_USERNAME/HyperInfer
export RUNNER_TOKEN=<token-from-step-2>
export RUNNER_NAME=hyperinfer-runner-$(hostname)

docker-compose up -d
```

### 5. Verify

```bash
# Check runner logs
docker-compose logs -f

# Verify runner appears in GitHub Settings → Actions → Runners
```

## Alternative Setup: Bridge Network Mode

If `network_mode: host` doesn't work (e.g., Docker Desktop on macOS/Windows):

### 1. Update docker-compose.yml

Comment out `network_mode: host` and uncomment the bridge network section.

### 2. Configure llama-server to listen on all interfaces

```bash
./llama-server \
    -hf unsloth/gemma-4-26B-A4B-it-GGUF:UD-Q4_K_XL \
    --host 0.0.0.0 \  # Important: listen on all interfaces
    --port 8001 \
    --temp 1.0 --top-p 0.95 --top-k 64
```

### 3. Update the workflow file (if needed)

The workflow is already configured to use llama.cpp's OpenAI-compatible API via the `LLAMA_API_BASE` environment variable. The default is `http://host.docker.internal:8001/v1` for Docker setups.

If you need to change the llama-server URL, edit `.github/workflows/pr_agent.yml` or set the environment variable in `docker-compose.yml`.

## Alternative Setup: Fully Containerized (Most Isolated)

Run both the GitHub runner AND llama-server in Docker.

### 1. Prepare Model Directory

```bash
mkdir -p ~/.local/share/llama-models
cd ~/.local/share/llama-models

# Download Gemma 4 GGUF
pip install huggingface-hub hf_transfer
hf download unsloth/gemma-4-26B-A4B-it-GGUF \
    --local-dir . \
    --include "*UD-Q4_K_XL*"
```

### 2. Update docker-compose.yml

Comment out Option 1 and uncomment Option 3 (the `llama-server` service).

### 3. Start Both Services

```bash
cd .github/runner
export REPO_URL=https://github.com/YOUR_USERNAME/HyperInfer
export RUNNER_TOKEN=<your-token>
export MODELS_DIR=$HOME/.local/share/llama-models

docker-compose up -d
```

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `REPO_URL` | Yes | - | Your GitHub repository URL |
| `RUNNER_TOKEN` | Yes | - | GitHub runner registration token |
| `RUNNER_NAME` | No | `hyperinfer-runner` | Name shown in GitHub |
| `LLAMA_API_BASE` | No | `http://host.docker.internal:8001/v1` | llama.cpp OpenAI-compatible API URL |
| `MODELS_DIR` | No | `./models` | Path to GGUF models (Option 3) |

## Security Considerations

1. **Docker Socket Access**: The runner has access to your Docker socket, meaning it can spawn containers. Only use this for trusted repositories.

2. **Network Isolation**: 
   - Host mode: Runner shares network namespace with host (least isolated)
   - Bridge mode: Runner has isolated network but can reach host (balanced)
   - Full Docker: Everything in containers (most isolated)

3. **Token Security**: The `RUNNER_TOKEN` is sensitive. Use `.env` file or secrets manager:
   ```bash
   # Create .env file (it's gitignored)
   echo "RUNNER_TOKEN=your-token-here" > .github/runner/.env
   docker-compose --env-file .env up -d
   ```

## Troubleshooting

### Runner can't connect to llama-server

```bash
# Test from inside the container
docker exec -it hyperinfer-github-runner curl http://localhost:8001/health

# If using bridge mode, use host IP
docker exec -it hyperinfer-github-runner curl http://host.docker.internal:8001/health
```

### Permission denied with Docker socket

```bash
# Add your user to docker group
sudo usermod -aG docker $USER
# Log out and back in
```

### Runner not appearing in GitHub

1. Check logs: `docker-compose logs -f`
2. Verify token hasn't expired (they're single-use)
3. Check if runner is already registered: `docker exec hyperinfer-github-runner cat /home/runner/.runner`

### Out of memory

Gemma 4 models need significant RAM:
- 26B-A4B: ~18GB for 4-bit
- 31B: ~20GB for 4-bit

If OOM:
- Use smaller model (E2B or E4B)
- Enable swap
- Use a machine with more RAM

## Updating the Runner

```bash
cd .github/runner
docker-compose down
docker-compose pull
docker-compose build --no-cache
docker-compose up -d
```

## Cleanup

```bash
cd .github/runner

# Stop and remove runner
docker-compose down

# Remove runner from GitHub (runner will unregister itself)
# Or manually remove from: Settings → Actions → Runners

# Clean up volumes
docker volume rm runner_cargo-cache runner_npm-cache runner_runner-work
```