#!/bin/bash
set -e

# Configuration from environment variables
REPO_URL=${REPO_URL:-""}
RUNNER_TOKEN=${RUNNER_TOKEN:-""}
RUNNER_NAME=${RUNNER_NAME:-"docker-runner-$(hostname)"}
RUNNER_WORKDIR=${RUNNER_WORKDIR:-"_work"}
RUNNER_GROUP=${RUNNER_GROUP:-"default"}
RUNNER_LABELS=${RUNNER_LABELS:-"self-hosted,Linux,X64"}

if [ -z "$REPO_URL" ]; then
    echo "Error: REPO_URL environment variable is required"
    exit 1
fi

if [ -z "$RUNNER_TOKEN" ]; then
    echo "Error: RUNNER_TOKEN environment variable is required"
    exit 1
fi

echo "=== GitHub Actions Runner Configuration ==="
echo "Repository: $REPO_URL"
echo "Runner Name: $RUNNER_NAME"
echo "Labels: $RUNNER_LABELS"
echo "=========================================="

# Configure the runner
cd /home/runner
if [ ! -f .runner ]; then
    echo "Configuring runner..."
    ./config.sh \
        --url "$REPO_URL" \
        --token "$RUNNER_TOKEN" \
        --name "$RUNNER_NAME" \
        --work "$RUNNER_WORKDIR" \
        --runnergroup "$RUNNER_GROUP" \
        --labels "$RUNNER_LABELS" \
        --unattended \
        --replace
    
    echo "Runner configured successfully"
else
    echo "Runner already configured"
fi

# Cleanup function for graceful shutdown
cleanup() {
    echo "Received shutdown signal, removing runner..."
    ./config.sh remove --token "$RUNNER_TOKEN" || true
    exit 0
}

# Trap SIGTERM and SIGINT for graceful shutdown
trap 'cleanup' SIGTERM SIGINT

echo "Starting runner..."
# Run the listener in the background and wait for it
./run.sh &
RUNNER_PID=$!

# Wait for the runner process
wait $RUNNER_PID