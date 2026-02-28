# ── Build stage ───────────────────────────────────────────────────────
FROM rust:1.82-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock* ./
COPY src/ src/
COPY config/ config/

# Build in release mode
RUN cargo build --release --bin gmv-agent

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -s /bin/bash gmv

COPY --from=builder /build/target/release/gmv-agent /usr/local/bin/gmv-agent
COPY --from=builder /build/config /home/gmv/.gmv-agent/config

RUN mkdir -p /home/gmv/.gmv-agent/data \
             /home/gmv/.gmv-agent/logs \
             /home/gmv/.gmv-agent/plugins \
    && chown -R gmv:gmv /home/gmv/.gmv-agent

USER gmv
WORKDIR /home/gmv

ENV HOME=/home/gmv
ENV GMV_DATA_DIR=/home/gmv/.gmv-agent/data

# Webhook server port
EXPOSE 8443

# Default: run in serve mode (scheduler + webhooks)
ENTRYPOINT ["gmv-agent"]
CMD ["serve", "--foreground"]
