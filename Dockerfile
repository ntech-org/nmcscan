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

# Create a minimal set of source files to satisfy Cargo for dependency caching.
RUN mkdir -p src migration/src && \
    echo "fn main() {}" > src/main.rs && \
    echo 'pub use sea_orm_migration::prelude::*; pub use sea_orm_migration::MigratorTrait; pub struct Migrator; #[async_trait::async_trait] impl MigratorTrait for Migrator { fn migrations() -> Vec<Box<dyn MigrationTrait>> { vec![] } }' > migration/src/lib.rs

# Cache dependencies (this downloads and builds all external crates like sea-orm, axum, etc.)
RUN cargo build --release

# CRITICAL: Remove the dummy source AND all metadata/fingerprints for local crates
# This forces Cargo to rebuild them from the real source we copy in the next step.
RUN rm -rf src migration target/release/nmcscan* target/release/deps/nmcscan* target/release/deps/libmigration* target/release/.fingerprint/migration* target/release/.fingerprint/nmcscan*

# Copy the real source code
COPY migration ./migration
COPY src ./src

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
