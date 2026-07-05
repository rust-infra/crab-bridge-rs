FROM rust:bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static

RUN cargo build --release

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/crabridge /usr/local/bin/crabridge

WORKDIR /app

EXPOSE 11435

VOLUME ["/app/data"]

ENV CRABRIDGE_CONFIG=/etc/crabbridge/crabbridge.toml \
    BRIDGE_ADDR=0.0.0.0:11435 \
    SESSION_DB=/app/data/crabbridge.db \
    LOG_LEVEL=info

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:11435/health >/dev/null || exit 1

ENTRYPOINT ["crabridge"]
CMD ["serve"]
