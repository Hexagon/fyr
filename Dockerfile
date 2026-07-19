# Multi-stage Dockerfile for Fyr
# Builds a minimal, efficient container image

# Stage 0: Frontend builder
FROM node:24-bookworm AS frontend-builder

WORKDIR /build/crates/ui/frontend

COPY crates/ui/frontend/package.json crates/ui/frontend/package-lock.json ./
RUN npm ci

COPY crates/ui/frontend ./
RUN npm run build

# Stage 1: Builder
FROM rust:bookworm AS builder

WORKDIR /build

# Copy workspace manifests first so dependency compilation can be cached.
COPY Cargo.toml Cargo.lock ./
COPY crates/types/Cargo.toml crates/types/Cargo.toml
COPY crates/downloader/Cargo.toml crates/downloader/Cargo.toml
COPY crates/server/Cargo.toml crates/server/Cargo.toml
COPY crates/ui/Cargo.toml crates/ui/Cargo.toml
COPY vendor/zim vendor/zim

# Prime Cargo's dependency layer with minimal crate sources.
RUN mkdir -p crates/types/src crates/downloader/src crates/server/src crates/ui/src \
  && touch crates/types/src/lib.rs crates/downloader/src/lib.rs crates/ui/src/lib.rs \
  && printf 'fn main() {}\n' > crates/server/src/main.rs

RUN cargo build --release --locked -p server --bin fyr

# Copy the real project contents after dependencies are cached.
COPY crates crates
COPY public public
COPY --from=frontend-builder /build/public/static /build/public/static

# Ensure Cargo sees copied sources as newer than the priming stub files.
RUN find crates -type f -exec touch {} +

RUN cargo build --release --locked -p server --bin fyr

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
