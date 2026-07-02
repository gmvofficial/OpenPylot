# ── Build stage ───────────────────────────────────────────────────────
FROM rust:1.93-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY src/ src/
COPY config/ config/

# Build in release mode.
# Cargo.toml sets `lto = true` (fat LTO) for the smallest local binary, but
# linking that way needs >4GB RAM. Override to thin LTO here so the image
# builds within typical Docker Desktop / CI memory limits (~3-4GB).
ENV CARGO_PROFILE_RELEASE_LTO=thin
ENV CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
RUN cargo build --release --bin pylot

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash pylot

COPY --from=builder /build/target/release/pylot /usr/local/bin/pylot
COPY --from=builder /build/config /home/pylot/.pylot/config

RUN mkdir -p /home/pylot/.pylot/data \
             /home/pylot/.pylot/logs \
             /home/pylot/.pylot/plugins \
    && chown -R pylot:pylot /home/pylot/.pylot

USER pylot
WORKDIR /home/pylot

ENV HOME=/home/pylot
ENV PYLOT_DATA_DIR=/home/pylot/.pylot/data

# Webhook server port
EXPOSE 8443

# Default: run in serve mode (scheduler + webhooks)
ENTRYPOINT ["pylot"]
CMD ["serve", "--foreground"]
