# syntax=docker/dockerfile:1.7
#
# Box Fraise Platform — multi-stage build (Hardening §8)
#
# Stage 1 builds the release binary plus sqlx-cli; stage 2 ships only the
# binary, sqlx-cli, and runtime libs. CMD runs migrations then starts the
# server so a fresh container can converge on whatever DATABASE_URL points at.
#
# Build:    docker build -t box-fraise-platform:dev .
# Run:      see docker-compose.yml `--profile full` (env vars must be set).

# ── Stage 1: builder ─────────────────────────────────────────────────────────
FROM rust:1.95-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Workspace files first so a code-only change reuses the dep cache layer.
COPY Cargo.toml Cargo.lock ./
COPY domain/       domain/
COPY integrations/ integrations/
COPY server/       server/

RUN cargo build --release --bin server

# sqlx-cli is needed at runtime to run migrations on container start.
# Build it in the same builder layer so the runtime stage stays slim.
RUN cargo install sqlx-cli \
        --no-default-features \
        --features postgres \
        --locked

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Server binary + sqlx-cli for migrations.
COPY --from=builder /app/target/release/server     /app/server
COPY --from=builder /usr/local/cargo/bin/sqlx      /usr/local/bin/sqlx
COPY --from=builder /app/server/migrations         /app/migrations

ENV RUST_LOG=info
EXPOSE 8080

# `sqlx migrate run` requires DATABASE_URL in the environment. The CMD chains
# migration → bind so the container is self-converging — operators don't run
# migrations as a separate step.
CMD ["sh", "-c", "sqlx migrate run --source /app/migrations && exec /app/server"]
