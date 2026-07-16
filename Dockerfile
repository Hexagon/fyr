# Multi-stage Dockerfile for Offline Nexus
# Builds a minimal, efficient container image

# Stage 1: Builder
FROM rust:1.75 as builder

WORKDIR /build

# Copy entire project
COPY . .

# Build release binary
RUN cargo build --release

# Stage 2: Runtime (minimal base image)
FROM debian:bookworm-slim

# Install runtime dependencies only (certificates for HTTPS)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 nexus

# Copy binary from builder
COPY --from=builder /build/target/release/nexus /usr/local/bin/nexus

# Set working directory
WORKDIR /data

# Change ownership to nexus user
RUN chown -R nexus:nexus /data

# Switch to non-root user
USER nexus

# Expose port
EXPOSE 8080

# Set environment variables
ENV DATA_DIR=/data
ENV RUST_LOG=info
ENV HOST=0.0.0.0

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -f http://localhost:8080/api/status || exit 1

# Run application
CMD ["nexus"]
