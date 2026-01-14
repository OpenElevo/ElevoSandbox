# Build stage for workspace-agent
FROM rust:1.83-slim-bookworm AS agent-builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY server ./server
COPY agent ./agent
COPY proto ./proto

# Build agent in release mode
RUN cargo build --release --package workspace-agent

# Runtime stage
FROM ubuntu:22.04

# Prevent interactive prompts during installation
ENV DEBIAN_FRONTEND=noninteractive

# Install base packages
RUN apt-get update && apt-get install -y \
    curl \
    wget \
    git \
    vim \
    nano \
    htop \
    procps \
    net-tools \
    iputils-ping \
    dnsutils \
    ca-certificates \
    gnupg \
    build-essential \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20.x
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Python 3.11
RUN apt-get update && apt-get install -y \
    python3.11 \
    python3.11-venv \
    python3-pip \
    && update-alternatives --install /usr/bin/python python /usr/bin/python3.11 1 \
    && update-alternatives --install /usr/bin/python3 python3 /usr/bin/python3.11 1 \
    && rm -rf /var/lib/apt/lists/*

# Create workspace directory
RUN mkdir -p /workspace
WORKDIR /workspace

# Create non-root user
RUN useradd -m -s /bin/bash workspace \
    && chown -R workspace:workspace /workspace

# Copy agent binary from builder
COPY --from=agent-builder /app/target/release/workspace-agent /usr/local/bin/workspace-agent
RUN chmod +x /usr/local/bin/workspace-agent

# Environment variables
ENV WORKSPACE_DIR=/workspace
ENV PATH="/usr/local/bin:$PATH"

# Default command - start the agent
CMD ["/usr/local/bin/workspace-agent"]
