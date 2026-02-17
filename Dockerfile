# ---- Build Stage ----
FROM rust:1.83-bookworm AS builder

# Install build dependencies for native crates (mozjpeg, etc.)
RUN apt-get update && apt-get install -y \
    cmake nasm pkg-config libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# Build release binaries
RUN cargo build --release --bin pixa --bin pixa-web

# ---- Runtime Stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/pixa /usr/local/bin/
COPY --from=builder /app/target/release/pixa-web /usr/local/bin/

# Copy web static assets
COPY --from=builder /app/crates/web/static /app/static

WORKDIR /app

EXPOSE 3000

# Default: run web server
CMD ["pixa-web"]
