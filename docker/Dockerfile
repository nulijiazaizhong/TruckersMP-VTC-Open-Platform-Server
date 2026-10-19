# VTC 服务端 · Rust · Docker 多阶段构建
# 形态：Axum + SQLite（预编译，避免容器内 C 编译）+ 本地存储
#
# 阶段 1：rust 构建（musl 静态链接，避免 glibc 依赖）
FROM rust:1.89-slim AS builder
WORKDIR /app

# 安装预编译 sqlite 所需：musl 目标下 libsqlite3-sys 用 system sqlite
# 这里用 musl + 系统 sqlite，避免 bundled C 编译
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev perl make cmake \
    && rustup target add x86_64-unknown-linux-musl \
    && rustup default stable && rm -rf /var/lib/apt/lists/*

COPY . .

# 构建（release + musl 静态）
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN cargo build --release --target x86_64-unknown-linux-musl -p vtc-server \
    && cp target/x86_64-unknown-linux-musl/release/vtc-server /app/vtc-server-bin

# 阶段 2：运行时（distroless/static，无 shell，非 root）
FROM gcr.io/distroless/static:nonroot
WORKDIR /app
COPY --from=builder /app/vtc-server-bin /app/vtc-server
COPY --from=builder /app/map-dlc /app/map-dlc
# 数据卷：DB + 存储
VOLUME ["/data"]
ENV PORT=8731 \
    DB_PATH=/data/vtc.db \
    STORAGE_DIR=/data/storage \
    MAP_DLC_DIR=/app/map-dlc \
    PUBLIC_BASE=http://127.0.0.1:8731

EXPOSE 8731
USER nonroot
# 健康检查：distroless/static 无 shell，靠平台层 TCP/HTTP 探测
# （K8s 用 tcpSocket:8731 或 httpGet:/api/v1/health；docker 用 compose 的 healthcheck 加 curl 镜像或平台 LB）
ENTRYPOINT ["/app/vtc-server"]
