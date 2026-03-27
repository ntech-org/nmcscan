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

# Copy dependency manifests
COPY Cargo.toml Cargo.lock ./
COPY migration/Cargo.toml ./migration/

# Create a minimal set of source files to satisfy Cargo for dependency caching
RUN mkdir -p src migration/src && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub struct Migrator; impl sea_orm_migration::MigratorTrait for Migrator { fn migrations() -> Vec<Box<dyn sea_orm_migration::MigrationTrait>> { vec![] } }" > migration/src/lib.rs

# Cache dependencies
RUN cargo build --release

# Remove dummy artifacts before copying real source
RUN rm -rf src migration target/release/deps/nmcscan* target/release/deps/migration*

# Copy the real source code
COPY src ./src
COPY migration ./migration

# Final build
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
