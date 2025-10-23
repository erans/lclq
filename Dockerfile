# Multi-stage Dockerfile for lclq
# Stage 1: Builder
# Using latest Rust for edition 2024 support
FROM rust:latest as builder

WORKDIR /usr/src/lclq

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations
COPY benches ./benches

# Build release binary
RUN cargo build --release --bin lclq

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd -m -u 1000 lclq

# Create directories for data and config
RUN mkdir -p /data /config && chown -R lclq:lclq /data /config

# Copy binary from builder
COPY --from=builder /usr/src/lclq/target/release/lclq /usr/local/bin/lclq

# Set ownership
RUN chown lclq:lclq /usr/local/bin/lclq

# Switch to non-root user
USER lclq

# Working directory
WORKDIR /app

# Expose ports
# 9324 - SQS HTTP API
# 8085 - Pub/Sub gRPC (future)
# 8086 - Pub/Sub HTTP (future)
# 9000 - Admin API
# 9090 - Metrics (Prometheus)
EXPOSE 9324 8085 8086 9000 9090

# Set environment variables
ENV RUST_LOG=info
ENV LCLQ_DATA_DIR=/data
ENV LCLQ_CONFIG_FILE=/config/lclq.toml

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/lclq", "health"]

# Default command
ENTRYPOINT ["/usr/local/bin/lclq"]
CMD ["start"]
