# Multi-stage Dockerfile for Fyr
# Builds a minimal, efficient container image

# Stage 1: Builder
FROM rust:stable AS builder

WORKDIR /build

# Copy entire project
COPY . .

# Build release binary
RUN cargo build --release -p server --bin fyr

# Stage 2: Runtime (minimal base image)
FROM debian:bookworm-slim

# Install runtime dependencies only (certificates + curl for healthcheck)
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m -u 1000 fyr

# Copy binary from builder
COPY --from=builder /build/target/release/fyr /usr/local/bin/fyr

# Copy static runtime assets (built frontend, reader bundle, default assets)
COPY --from=builder /build/public /app/public

# Set working directory
WORKDIR /app

# Prepare writable runtime data directory
RUN mkdir -p /data && chown -R fyr:fyr /app /data

# Switch to non-root user
USER fyr

# Expose port
EXPOSE 8080

# Set environment variables
ENV DATA_DIR=/data
ENV RUST_LOG=info
ENV FYR_HOST=0.0.0.0
ENV FYR_PORT=8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD curl -fsS http://localhost:8080/api/status || exit 1

# Run application
CMD ["fyr"]
