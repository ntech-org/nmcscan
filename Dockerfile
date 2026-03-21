# Build stage
FROM rust:1.80-slim as builder

WORKDIR /usr/src/nmcscan

# Install dependencies for building
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    sqlite3 \
    &> /dev/null

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main to pre-build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

# Copy source
COPY src ./src
COPY assets ./assets

# Build for release
RUN touch src/main.rs && cargo build --release

# Run stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    sqlite3 \
    &> /dev/null && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy binary
COPY --from=builder /usr/src/nmcscan/target/release/nmcscan /app/nmcscan
# Copy assets
COPY assets /app/assets

# Create data directory for persistent DB
RUN mkdir /app/data

# Default environment variables
ENV RUST_LOG=info
ENV API_KEY=""

EXPOSE 3000

# Run with data volume for DB
CMD ["/app/nmcscan", "--log-level", "${RUST_LOG}"]
