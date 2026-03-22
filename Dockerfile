# Build stage
FROM rust:1.85-slim-bookworm as builder

# Install build tools (sqlite3 only for CLI/testing if needed, gcc/make for other C deps)
RUN apt-get update && apt-get install -y \
    sqlite3 \
    gcc \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/nmcscan

# Copy dependency manifest and build a dummy main.rs for caching
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -f target/release/deps/nmcscan*

# Copy the real source code and build it
COPY src ./src
RUN cargo build --release

# Final runtime stage (using a very small distroless-like image for security and speed)
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies (sqlite3 if needed for the db)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    sqlite3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from the builder stage
COPY --from=builder /usr/src/nmcscan/target/release/nmcscan /app/nmcscan

# Ensure data directory exists
RUN mkdir -p /app/data/maxmind

# Default command
CMD ["/app/nmcscan"]
