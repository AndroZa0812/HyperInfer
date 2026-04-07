FROM ubuntu:22.04

# Install dependencies
RUN apt-get update && apt-get install -y \
    curl \
    git \
    jq \
    sudo \
    iputils-ping \
    netcat \
    docker.io \
    && rm -rf /var/lib/apt/lists/*

# Create runner user
RUN useradd -m -s /bin/bash runner && \
    usermod -aG sudo runner && \
    usermod -aG docker runner && \
    echo "runner ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# Set up runner directory
WORKDIR /home/runner
USER runner

# Download and extract GitHub Actions runner
# Version and architecture can be overridden via build args
ARG RUNNER_VERSION=2.333.1
ARG RUNNER_ARCH=linux-x64
RUN curl -o actions-runner.tar.gz -L \
    https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-${RUNNER_ARCH}-${RUNNER_VERSION}.tar.gz && \
    tar xzf actions-runner.tar.gz && \
    rm actions-runner.tar.gz

# Install runner dependencies
USER root
RUN /home/runner/bin/installdependencies.sh

# Create entrypoint script
COPY docker-entrypoint.sh /home/runner/
RUN chmod +x /home/runner/docker-entrypoint.sh

USER runner

ENTRYPOINT ["/home/runner/docker-entrypoint.sh"]