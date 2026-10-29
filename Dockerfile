# syntax=docker/dockerfile:1

# Stage 1: Base image with cargo-chef for deterministic dependency caching
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

# Stage 2: Planner - Compute lock-file & workspace dependency recipe
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder - Compile cached dependencies, then target microservice
FROM chef AS builder
ARG SERVICE_NAME=gateway-service

COPY --from=planner /app/recipe.json recipe.json

# Pre-compile all workspace dependencies (cached layer unless Cargo.toml/Cargo.lock changes)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo chef cook --release --recipe-path recipe.json

# Copy workspace source code and build the requested service binary
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release --package "${SERVICE_NAME}" && \
    cp "/app/target/release/${SERVICE_NAME}" /app/service

# Stage 4: Runtime - Minimal Distroless image (~20MB, non-root, no shell)
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app

# Copy compiled binary and Fluent i18n locale bundles directly with nonroot ownership
COPY --chown=nonroot:nonroot --from=builder /app/service /app/service
COPY --chown=nonroot:nonroot --from=builder /app/libs/common/locales /app/locales

# Run as non-root user (UID 65532)
USER nonroot:nonroot

# Expose default HTTP port
EXPOSE 8080

ENTRYPOINT ["/app/service"]
