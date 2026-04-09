# Shared workspace builder stage - builds all dependencies once
FROM rust:1-slim-trixie AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy workspace Cargo.toml
COPY Cargo.toml ./

# Copy all member Cargo.toml files for workspace resolution
COPY packages/shared/Cargo.toml packages/shared/
COPY packages/api/Cargo.toml packages/api/
COPY packages/scanner/Cargo.toml packages/scanner/
COPY migration/Cargo.toml migration/

# Create stub files for dependency resolution
RUN mkdir -p packages/shared/src packages/api/src packages/scanner/src migration/src && \
    echo "pub fn main() {}" > packages/shared/src/lib.rs && \
    echo "fn main() {}" > packages/api/src/main.rs && \
    echo "fn main() {}" > packages/scanner/src/main.rs && \
    echo "pub fn main() {}" > migration/src/lib.rs

# Fetch dependencies (cached unless Cargo.toml files change)
RUN cargo fetch

# Copy actual source code
COPY packages/shared/src packages/shared/src
COPY packages/api/src packages/api/src
COPY packages/scanner/src packages/scanner/src
COPY migration/src migration/src

# Build entire workspace - all shared dependencies compiled once
RUN cargo build --workspace --release

# API service runtime
FROM debian:trixie-slim AS api-runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates wget && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nmcscan-api /usr/local/bin/
COPY exclude.conf /app/
COPY honeypots.conf /app/
COPY dashboard /app/dashboard

WORKDIR /app

EXPOSE 3000

ENV RUST_LOG=info
ENV DATABASE_URL=postgres://nmcscan:nmcscan_secret@postgres:5432/nmcscan
ENV LISTEN_ADDR=0.0.0.0:3000

CMD ["nmcscan-api"]

# Scanner service runtime
FROM debian:trixie-slim AS scanner-runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/nmcscan-scanner /usr/local/bin/
COPY exclude.conf /app/
COPY honeypots.conf /app/

WORKDIR /app

ENV RUST_LOG=info
ENV DATABASE_URL=postgres://nmcscan:nmcscan_secret@postgres:5432/nmcscan

CMD ["nmcscan-scanner"]
