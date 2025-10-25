# Multi-stage Dockerfile for lclq - Alpine Linux
# Produces a minimal production image (~30-40MB total)

# Stage 1: Builder
FROM rust:alpine as builder

# Build arguments for metadata
ARG VERSION=0.1.0
ARG VCS_REF
ARG BUILD_DATE

# Install build dependencies for Alpine
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    protobuf-dev \
    sqlite-dev

WORKDIR /usr/src/lclq

# Copy manifest files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy proto files (required for build.rs)
COPY proto ./proto

# Copy build script
COPY build.rs ./

# Copy source code
COPY src ./src
COPY migrations ./migrations
COPY benches ./benches

# Build release binary with static linking for Alpine
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build --release --bin lclq

# Stage 2: Runtime (Alpine)
FROM alpine:latest

# Build arguments for labels
ARG VERSION=0.1.0
ARG VCS_REF
ARG BUILD_DATE

# Add OCI labels for better metadata
LABEL org.opencontainers.image.title="lclq" \
      org.opencontainers.image.description="Lightweight local queue service compatible with AWS SQS and GCP Pub/Sub" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.authors="lclq Contributors" \
      org.opencontainers.image.url="https://github.com/erans/lclq" \
      org.opencontainers.image.source="https://github.com/erans/lclq" \
      org.opencontainers.image.vendor="lclq" \
      org.opencontainers.image.licenses="MIT OR Apache-2.0" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.created="${BUILD_DATE}"

# Install runtime dependencies
# - ca-certificates: SSL/TLS certificates for HTTPS
# - libgcc: Required for Rust binaries
# - sqlite: CLI tool for debugging SQLite backend
RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    sqlite

# Create a non-root user for security
RUN adduser -D -u 1000 lclq

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
# 9324 - AWS SQS HTTP API
# 8085 - GCP Pub/Sub gRPC API
# 8086 - GCP Pub/Sub HTTP/REST API
# 9000 - Admin API
# 9090 - Metrics (Prometheus)
EXPOSE 9324 8085 8086 9000 9090

# Set environment variables for containerized deployment
ENV RUST_LOG=info \
    LCLQ_DATA_DIR=/data \
    LCLQ_CONFIG_FILE=/config/lclq.toml \
    LCLQ_BIND_ADDRESS=0.0.0.0

# Health check using the built-in health command
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD ["/usr/local/bin/lclq", "health"]

# Default command
ENTRYPOINT ["/usr/local/bin/lclq"]
CMD ["start"]
