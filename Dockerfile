# VTC 服务端 · Rust · Docker 多阶段构建
# 形态：Axum + SQLite（预编译，避免容器内 C 编译）+ 本地存储
#
# 阶段 1：rust 构建（musl 静态链接，避免 glibc 依赖）
FROM rust:1.89-slim AS builder
WORKDIR /app

# 构建依赖：
#   - build-essential     提供 gcc（基础工具链；musl-gcc 也依赖它）
#   - musl-tools          提供 musl-gcc —— 用 musl 头编译 C 源码的关键。
#                         为什么不能直接用 glibc 的 gcc：glibc 的 gcc 会把 SQLite C
#                         编出引用 glibc 专属符号（如 fcntl64）的 .o，链进 musl 二进制
#                         时 undefined reference to `fcntl64'（musl 只有 fcntl，无 64 变体）。
#                         musl-gcc 用 musl 头编译，产出的 C 对象引用 musl 认识的符号，
#                         静态链接干净。
# 注意：rusqlite 开了 bundled（见 workspace Cargo.toml），SQLite 用自带 C 源码现场编译，
#       不再依赖系统 libsqlite3 —— 因此**不需要** libsqlite3-dev/pkg-config。
#       这正是修掉运行时 SIGSEGV(code 139) 的关键：musl 静态下系统 libsqlite3.a
#       混入的 glibc 启动对象不再进入二进制。
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential musl-tools \
    && rm -rf /var/lib/apt/lists/*

# 单独成一步装 musl target 并验证：装失败会**立即**停在这一步，
# 而不是漏到 cargo build 里才报 E0463，方便定位。
RUN rustup target add x86_64-unknown-linux-musl \
    && rustup target list --installed | grep -q 'x86_64-unknown-linux-musl'

COPY . .

# 构建（release + musl 静态）
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
# bundled sqlite 用 cc-rs 编译 C 源码。必须用 **musl-gcc**（musl 头），不能用 glibc 的 gcc：
#   glibc gcc 编 SQLite C 会产出引用 glibc 专属符号（fcntl64 等）的对象 →
#   最终 musl 静态链接 undefined reference to `fcntl64'。
#   musl-gcc（musl-tools 提供）用 musl 头编译，C 对象只引用 musl 认识的符号，链接干净。
# 之前踩的坑：CC=gcc 能通过 C 编译但链接报 fcntl64 未定义；CC_x86_64-unknown-linux-musl
# 因变量名带连字符不被 shell 导出（cc-rs 读到 None）。musl-gcc 名字无连字符、shell 合法。
ENV CC=musl-gcc
RUN cargo build --release --target x86_64-unknown-linux-musl -p vtc-server \
    && cp target/x86_64-unknown-linux-musl/release/vtc-server /app/vtc-server-bin

# 阶段 2：运行时（distroless/static，无 shell，非 root）
FROM gcr.io/distroless/static:nonroot
WORKDIR /app
COPY --from=builder /app/vtc-server-bin /app/vtc-server
# map-dlc 地图下载模板：现已随仓库一起放在 vtc-rs/map-dlc/（独立部署自包含，
# 不依赖任何上层目录）。COPY 进镜像 /app/map-dlc，运行时 MAP_DLC_DIR 读它。
# 如需热更新模板而不重建镜像，可在 compose 里用 bind mount 覆盖 /app/map-dlc。
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
