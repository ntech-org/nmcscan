# NMCScan Scanner Service Dockerfile
FROM rust:1-slim-trixie AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy workspace files (all members for workspace resolution)
COPY Cargo.toml ./
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

# Build dependencies (cached layer)
RUN cargo fetch

# Copy actual source code
COPY packages/shared/src packages/shared/src
COPY packages/api/src packages/api/src
COPY packages/scanner/src packages/scanner/src
COPY migration/src migration/src

# Build scanner service
RUN cargo build -p nmcscan-scanner --release && \
    cp /app/target/release/nmcscan-scanner /usr/local/bin/

# Runtime image
FROM debian:trixie-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/nmcscan-scanner /usr/local/bin/
COPY exclude.conf /app/
COPY honeypots.conf /app/

WORKDIR /app

ENV RUST_LOG=info
ENV DATABASE_URL=postgres://nmcscan:nmcscan_secret@postgres:5432/nmcscan

CMD ["nmcscan-scanner"]
