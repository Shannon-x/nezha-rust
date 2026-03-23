# ─── Multi-stage Rust build ───
FROM rust:1.83-bookworm AS builder

RUN apt-get update && apt-get install -y protobuf-compiler cmake pkg-config && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .

# 利用 Docker layer 缓存依赖
RUN --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release && \
    cp /app/target/release/nezha-dashboard /usr/local/bin/nezha-dashboard

# ─── 最小运行时镜像 ───
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates tzdata curl && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd -r nezha && useradd -r -g nezha nezha && \
    mkdir -p /data && chown nezha:nezha /data

COPY --from=builder /usr/local/bin/nezha-dashboard /usr/local/bin/nezha-dashboard

USER nezha
WORKDIR /data

ENV TZ=Asia/Shanghai \
    NZ_LISTEN_PORT=8008 \
    NZ_DATABASE_PATH=/data/sqlite.db

EXPOSE 8008 5555

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8008/api/v1/setting || exit 1

ENTRYPOINT ["nezha-dashboard"]
CMD ["-c", "/data/config.yaml"]
