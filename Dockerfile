# Build dashboard
FROM oven/bun:latest as dashboard-builder
WORKDIR /usr/src/dashboard
COPY dashboard/package.json dashboard/bun.lock ./
RUN bun install --force
COPY dashboard/ .
RUN bun run build

# Build stage
FROM rust:slim-bookworm as builder
WORKDIR /usr/src/nmcscan

# Install dependencies for building (OpenSSL, SQLite, and build tools)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    sqlite3 \
    gcc \
    perl \
    make \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
COPY src ./src
# Assets are copied from dashboard-builder
COPY --from=dashboard-builder /usr/src/dashboard/build ./assets
RUN touch src/main.rs && cargo build --release

# Run stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates sqlite3 &> /dev/null && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /usr/src/nmcscan/target/release/nmcscan /app/nmcscan
COPY --from=builder /usr/src/nmcscan/assets /app/assets
COPY exclude.conf /app/exclude.conf
RUN mkdir /app/data
ENV RUST_LOG=info
ENV API_KEY=""
EXPOSE 3000
CMD ["/app/nmcscan", "--api-key", "${API_KEY}"]
