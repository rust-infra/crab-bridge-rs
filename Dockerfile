# China Docker registry mirror (DaoCloud). Override for non-CN builds:
#   docker build --build-arg DOCKER_REGISTRY= .
ARG DOCKER_REGISTRY=docker.m.daocloud.io/library/

FROM ${DOCKER_REGISTRY}rust:bookworm AS builder

WORKDIR /app

# Use China crates.io mirror (see .cargo/config.toml).
COPY .cargo/config.toml .cargo/config.toml
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY static ./static

RUN cargo build --release --bin crabridge

FROM ${DOCKER_REGISTRY}debian:bookworm-slim AS runtime

# Use Aliyun Debian mirror for faster apt downloads in China.
RUN set -eux; \
    if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
      sed -i 's|deb.debian.org|mirrors.aliyun.com|g; s|security.debian.org|mirrors.aliyun.com|g' \
        /etc/apt/sources.list.d/debian.sources; \
    elif [ -f /etc/apt/sources.list ]; then \
      sed -i 's|deb.debian.org|mirrors.aliyun.com|g; s|security.debian.org|mirrors.aliyun.com|g' \
        /etc/apt/sources.list; \
    fi; \
    apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/crabridge /usr/local/bin/crabridge
COPY crabbridge.docker.toml /etc/crabbridge/crabbridge.toml

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
