# Workspace base image with agent
# Uses pre-built binary (compiled with docker_complie.sh for glibc compatibility)
FROM ubuntu:24.04

LABEL maintainer="Elevo Team"
LABEL org.opencontainers.image.source="https://github.com/OpenElevo/ElevoSandbox"
LABEL org.opencontainers.image.description="Elevo Workspace Base Image with Agent"

# Install development tools commonly needed in workspaces
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    git \
    make \
    gcc \
    g++ \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create workspace directory
RUN mkdir -p /workspace

# Copy pre-built agent binary
COPY target/release/workspace-agent /usr/local/bin/workspace-agent
RUN chmod +x /usr/local/bin/workspace-agent

# Copy systemd service file (for reference)
COPY images/workspace-agent.service /etc/systemd/system/workspace-agent.service

WORKDIR /workspace

# Environment variables
ENV WORKSPACE_DIR=/workspace
ENV PATH="/usr/local/bin:$PATH"
ENV LANG=en_US.UTF-8
ENV LC_ALL=en_US.UTF-8

# Default command - start the agent
CMD ["/usr/local/bin/workspace-agent"]
