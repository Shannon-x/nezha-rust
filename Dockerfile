# ─── 阶段1: 构建前端 + 下载 GeoIP ───
FROM node:20-bookworm-slim AS frontend

RUN apt-get update && apt-get install -y git curl && rm -rf /var/lib/apt/lists/*

WORKDIR /frontend

# 构建 Admin 前端
RUN git clone --depth 1 https://github.com/nezhahq/admin-frontend.git admin && \
    cd admin && npm install --legacy-peer-deps && npm run build

# 构建 User 前端 (nezha-dash)
RUN git clone --depth 1 https://github.com/hamster1963/nezha-dash.git nezha-dash && \
    cd nezha-dash && npm install --legacy-peer-deps && npm run build

# 下载 GeoIP 数据库
ARG IPINFO_TOKEN=""
RUN if [ -n "$IPINFO_TOKEN" ]; then \
      curl -fsSL -o geoip.db "https://ipinfo.io/data/free/country.mmdb?token=${IPINFO_TOKEN}"; \
    else \
      echo "IPINFO_TOKEN not set, skipping GeoIP download" && touch geoip.db; \
    fi

# ─── 阶段2: 构建 Rust 后端 ───
FROM rustlang/rust:nightly-bookworm AS builder

WORKDIR /app
COPY . .

RUN --mount=type=cache,target=/app/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release && \
    cp /app/target/release/nezha-dashboard /usr/local/bin/nezha-dashboard

# ─── 阶段3: 最小运行时镜像 ───
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates tzdata curl && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir -p /data /opt/nezha/resource

COPY --from=builder /usr/local/bin/nezha-dashboard /usr/local/bin/nezha-dashboard
COPY --from=frontend /frontend/admin/dist /opt/nezha/resource/admin
COPY --from=frontend /frontend/nezha-dash/dist /opt/nezha/resource/user
COPY --from=frontend /frontend/nezha-dash/dist /opt/nezha/resource/
COPY --from=frontend /frontend/geoip.db /opt/nezha/resource/geoip.db

WORKDIR /data

ENV TZ=Asia/Shanghai \
    NZ_LISTEN_PORT=8008 \
    NZ_DATABASE_PATH=/data/sqlite.db \
    NZ_RESOURCE_DIR=/opt/nezha/resource

EXPOSE 8008 5555

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -sf http://localhost:8008/api/v1/setting || exit 1

ENTRYPOINT ["nezha-dashboard"]
CMD ["-c", "/data/config.yaml"]

