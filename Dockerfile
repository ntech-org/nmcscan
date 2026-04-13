# === DEPENDENCY PLANNING ===
# Use pre-built cargo-chef image so we don't install it every build
FROM lukemathwalker/cargo-chef:latest-rust-slim-trixie AS chef
WORKDIR /app

# Analyze the workspace and produce a recipe file describing all dependencies
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# === DEPENDENCY BUILDING ===
# Build ONLY third-party dependencies from the recipe. This layer is cached
# until any Cargo.toml changes — source code changes don't affect it.
FROM chef AS cacher
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --workspace --release --recipe-path recipe.json

# === APPLICATION BUILD ===
# Copy compiled deps from cacher, then copy source and build. Only your
# actual .rs files get recompiled when they change.
FROM chef AS builder
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Bring in pre-compiled dependency artifacts
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo

# Copy everything (source code + Cargo.toml files)
COPY . .

# Build workspace — deps already compiled, only app code compiles
RUN cargo build --workspace --release

# === API RUNTIME ===
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

# === SCANNER RUNTIME ===
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
