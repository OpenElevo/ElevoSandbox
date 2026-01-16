# Simple test image - copies pre-built agent into existing local image
FROM rust:1.85

# Copy pre-built static agent binary (musl)
COPY target/x86_64-unknown-linux-musl/release/workspace-agent /usr/local/bin/workspace-agent
RUN chmod +x /usr/local/bin/workspace-agent

# Create workspace directory
RUN mkdir -p /workspace
WORKDIR /workspace

# Environment variables
ENV WORKSPACE_DIR=/workspace
ENV PATH="/usr/local/bin:$PATH"

# Default command - start the agent
CMD ["/usr/local/bin/workspace-agent"]
