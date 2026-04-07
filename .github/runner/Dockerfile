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
# Version will be passed as build arg
ARG RUNNER_VERSION=2.322.0
RUN curl -o actions-runner.tar.gz -L \
    https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz && \
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