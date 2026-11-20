# syntax=docker/dockerfile:1.7
# Multi-stage ARM64 production image for `afterlifed` (ECS Fargate Graviton)

FROM rust:1.86-slim-bookworm AS builder
WORKDIR /src

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace manifests & backend crates (desktop GPUI crate is excluded from server build)
COPY Cargo.toml Cargo.lock ./
COPY crates/afterlife-core crates/afterlife-core
COPY crates/afterlife-protocol crates/afterlife-protocol
COPY crates/afterlife-storage crates/afterlife-storage
COPY crates/afterlife-daemon crates/afterlife-daemon
COPY crates/afterlife-cli crates/afterlife-cli
COPY db db

# Build only server & CLI binaries without pulling GUI/Metal crates
RUN cargo build --release -p afterlife-daemon -p afterlife-cli

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app afterlife

WORKDIR /app
COPY --from=builder /src/target/release/afterlifed /usr/local/bin/afterlifed
COPY --from=builder /src/target/release/afterlife /usr/local/bin/afterlife
COPY config/production.example.toml /app/config/production.example.toml
COPY db/migrations /app/db/migrations

USER afterlife
EXPOSE 4319
ENV AFTERLIFE_CONFIG=/app/config/production.example.toml

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:4319/v1/health || exit 1

ENTRYPOINT ["/usr/local/bin/afterlifed"]
