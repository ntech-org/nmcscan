# Build stage
FROM rust:1.94.0-slim-bookworm as builder

# Install build tools and lld linker for faster linking
RUN apt-get update && apt-get install -y \
    clang \
    lld \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/nmcscan

# Set RUSTFLAGS to use lld linker
ENV RUSTFLAGS="-C link-arg=-fuse-ld=lld"

# Copy dependency manifest and build dependencies separately for caching
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/nmcscan*

# Copy the real source code and build the application
COPY src ./src
RUN cargo build --release

# Final runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/nmcscan/target/release/nmcscan /app/nmcscan

# Ensure data directory exists
RUN mkdir -p /app/data/maxmind

# Default command
CMD ["/app/nmcscan"]
