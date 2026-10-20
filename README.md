# VTC 后端 · Rust 重写

> 把原 .NET 10 后端用 **Rust** 重写，**同一份代码双编译**：
> - **Docker 自部署**（Axum + SQLite + 本地/S3 + 内存推送）—— 自部署优先
> - **Cloudflare Workers**（worker crate + D1 + R2 + Durable Objects）—— 无服务器形态
>
> 规划依据：`ETS2 TruckersMP VTC 开源管理平台.md`（v0.3）§2.4 / §4 / §12。
> 触发原因：Cloudflare Workers/Pages 对 C# 非原生支持（只能靠未产品化的 WASM Native AOT），
> 对 Rust 是官方一等支持 → 按"用 Rust 重写"决策执行。

## 架构：可插拔后端，一份逻辑双编译

```
vtc-rs/
├─ Cargo.toml                  # workspace
├─ crates/
│  ├─ vtc-core/                # ★ 共享核心（两个二进制都链接）
│  │   └─ DTO + 常量 + 错误码 + Domain(13实体)
│  │   └─ backend::{Db, Storage, Push} trait（可插拔后端抽象）
│  │   └─ auth（JWT HS256 手写 + PBKDF2 手写，无 ring C 依赖）
│  │   └─ service（Auth/Member/Save/DLC/Content/Rate —— 后端无关）
│  ├─ vtc-server/              # Docker 形态
│  │   └─ SqliteDb(rusqlite) + LocalStorage + InMemPush + Axum 路由
│  └─ vtc-worker/              # Cloudflare 形态
│       └─ D1Db(worker::d1) + R2Storage + DoPush + worker.rs 入口
│       └─ wrangler.jsonc + migrations/
├─ map-dlc/                    # 复用上层模板
├─ docker/                     # Dockerfile + docker-compose
└─ .env.example
```

**关键点**：`vtc-core` 的 service 只依赖 `Db`/`Storage`/`Push` 三个 trait（`Arc<dyn ...>`），
不依赖具体实现。Docker 注入 SQLite/本地/内存，CF 注入 D1/R2/DO，业务逻辑零重写。

## 构建

### Docker 形态（自部署）

```bash
# 前置：rustup + 预编译 sqlite（沙箱下 MSVC cl 无法被 cc-rus 调用）
# 本机 Windows：
$env:SQLITE3_LIB_DIR = "<repo>/prebuilt"   # 指向 sqlite3.lib
$env:SQLITE3_STATIC = "1"
cargo build -p vtc-server

# 容器：
docker compose -f docker/docker-compose.yml up -d
```

### Cloudflare Workers 形态

完整步骤（建 D1/R2 → 填 wrangler.jsonc → 上传 DLC 模板到 R2 → 构建 → deploy → 验证）
见下方 **[Cloudflare Workers 部署](#cloudflare-workers-部署)**。最短路径：

```bash
cd crates/vtc-worker
cargo build --target wasm32-wasip2 --release   # 产物：vtc_worker.wasm（release 约 22 MB）
wrangler deploy
```

## 与 .NET 版的契约对齐

- REST 路由 / DTO 字段（camelCase）/ 错误码（1001-1008）/ 状态值 —— **与 VTC.Shared 完全一致**，
  现有 Avalonia 客户端**无需改动**即可对接 Rust 后端。
- 地图 DLC 模板文件（`map-dlc/1.61.ets2.txt` 10 条 / `ats` 20 条）复用。
- 种子：admin / `ChangeMe123!`（`ADMIN_INITIAL_PASSWORD` 覆盖）、默认车队。

## 双目标 async 边界（架构核心，已落地）

Docker/axum 的 `Handler` 要求 future 是 **Send**；Cloudflare worker 的 D1/R2 绑定是
`Rc`/`Arc<D1Database>`（**非 Send**，wasm 单线程）。二者无法共享同一套 `Arc<dyn Db>`。

**解法（已实现）**：
1. **service 层全部泛型**——`ConfigService<D: Db>`、`SaveService<D: Db, S: Storage>` 等，
   持有具体 `Arc<D>`/`Arc<S>`，**不持 `Arc<dyn Db>`**。Send 边界由具体类型 + trait 的 Send 开关决定。
2. **后端 trait 按 vtc-core 的 cargo feature `worker` 门控**（cargo feature 按依赖边生效）：
   - `vtc-server` 依赖 vtc-core（默认）→ 三个 trait 用 `#[async_trait]`（**Send**）→ axum 可用。
   - `vtc-worker` 依赖 vtc-core 开 `features=["worker"]` → 三个 trait 用 `#[async_trait(?Send)]`
     → D1/R2（非 Send）可用。每个 trait 有两个 cfg 孪生定义（签名一致，仅 Send 开关不同）。

service 代码两种 feature 下**完全相同**——这是"一份代码双编译"在 async 边界上的真正落地。

## 其它边界

- **推送**：Docker 用内存存根、CF 用 Durable Objects 骨架，**WebSocket 真实推送在 P1**。
- **S3/PostgreSQL**：Docker 默认 SQLite + 本地存储；S3/PG 是 P1（接口已留）。
- **C 依赖规避**：手写 JWT/PBKDF2（不拉 jsonwebtoken/pbkdf2 → 避免 ring C 编译）；
  SQLite 用预编译库（不 bundled）。这是为适应无 C 编译器环境的工程取舍，生产可换回 bundled。

## Cloudflare Workers 部署

worker 形态**零 C 依赖**（D1/R2 走 CF 托管，无 SQLite、无本地存储），`cargo build` 直接出 wasm，
不需要 Docker 形态那套预编译 SQLite。**一条代码、一套服务逻辑**，与 Docker 完全共享。

### 前置：本地资源（一次性）

在 `crates/vtc-worker/` 下执行（需 `wrangler`，`npm i -g wrangler` 或 `npm i -g wrangler@3`）：

```bash
cd crates/vtc-worker

# 1) 创建 D1（= SQLite 方言的关系型数据库；返回 database_id）
wrangler d1 create vtc
#    → 把返回的 database_id 填进 wrangler.jsonc 的 bindings[DB].database_id

# 2) 创建 R2 桶（对象存储：存档 + DLC 模板都放这）
wrangler r2 bucket create vtc-saves

# 3) 应用 D1 建表迁移（13 张表）
wrangler d1 migrations apply DB
```

### 填 wrangler.jsonc

- `bindings[DB].database_id`：上一步 `wrangler d1 create` 的值。
- `bindings[BUCKET].bucket_name`：R2 桶名（`vtc-saves`）。
- `routes[].zone_id`：你的 Cloudflare zone id（`wrangler whoami` 可查）；自定义域名场景填
  `/api/*` 与 `/storage/*` 两条路由（worker 同时服务两类路径，见 `src/lib.rs` 路由分发）。
- `vars`：非机密配置（`TEAM_NAME` / `UPDATE_CHANNEL` / `PUBLIC_BASE` / `AUTH_REQUIRE_TMP` …）。
  **`JWT_SECRET` 不写 vars**，用 secret：

```bash
wrangler secret put JWT_SECRET          # 生产必须 ≥32 字符
wrangler secret put VTC_STORAGE_SIGN_KEY # 存储中转 HMAC 签名密钥
```

### 上传 DLC 模板到 R2

worker 无文件系统，DLC 地图模板（`map-dlc/1.61.ets2.txt` 等）改存 **R2 的 `dlc-templates/` 前缀**
（`/dlc/map-template` 与 `/dlc/disable` 从这里读）。把上层 `map-dlc/` 的模板文件批量上传：

```bash
# ETS2 1.61（10 条）
wrangler r2 object put dlc-templates/1.61.ets2.txt --file <上层>/map-dlc/1.61.ets2.txt
# ATS 1.61（20 条）
wrangler r2 object put dlc-templates/1.61.ats.txt  --file <上层>/map-dlc/1.61.ats.txt
```

> 客户端按 `<major>.<game>.txt` 取模板；加新游戏版本时把对应模板 `put` 到 `dlc-templates/` 即可，
> 无需改 worker 代码。

### 构建 + 部署

```bash
cd crates/vtc-worker
# 产物：target/wasm32-wasip2/release/vtc_worker.wasm（release 约 22 MB，CF 限额内）
cargo build --target wasm32-wasip2 --release
wrangler deploy
#    → https://vtc-server.<你的子域>.workers.dev
```

### 验证（部署后）

```bash
B=<你的 workers.dev 域名>
curl $B/api/v1/health                        # ok
curl $B/api/v1/config/public                 # 车队品牌（无鉴权）
curl $B/api/v1/dlc/map-template?game=ets2&major=1.61   # R2 模板（10 条）
# 登录拿 token（admin / ChangeMe123!，可用 wrangler secret put 改 ADMIN_INITIAL_PASSWORD）
curl -X POST $B/api/v1/auth/login -H 'content-type: application/json' \
     -d '{"username":"admin","password":"ChangeMe123!"}'
# 带 Bearer 调管理端点（/members /saves /audit …）
```

### 形态对照（同一份 service，两种后端）

| 关注点 | Docker 形态（`vtc-server`） | CF Workers 形态（`vtc-worker`） |
|---|---|---|
| 数据库 | SQLite（`rusqlite`，本地文件 `vtc.db`） | **D1**（CF 托管，`db::D1Db` 实现 `Db`） |
| 对象存储 | 本地磁盘（`LocalStorage`） | **R2**（`R2Storage`；预签名 URL → worker 中转 HMAC 签名） |
| DLC 模板 | 本地 `map-dlc/*.txt` 文件 | **R2 `dlc-templates/*.txt`**（`parse_template` 解析） |
| 推送 | 内存 `InMemPush`（存根） | Durable Objects 骨架（存根） |
| 路由 | Axum（`{id}` 模板） | `match (path, method)` 段解析（无 axum） |
| async | `#[async_trait]`（Send） | `#[async_trait(?Send)]`（wasm 单线程） |
| 部署 | `docker compose up` | `wrangler deploy` |

## 状态

- **P0 全量 API**：Docker 形态编译绿 + 运行时全链路冒烟通过（health/config/public/login/members CRUD/
  saves 上传凭证+发布+下载凭证+取消+删除/announcements CRUD/events CRUD/dlc map-template+required+
  report+disable+restore/audit 列表+过滤/存储中转 PUT+GET/401 鉴权边界）。
- **Cloudflare Workers 形态**：编译绿（`vtc_worker.wasm` 22 MB），P0 全量路由与 Docker 同源
  （共享 vtc-core service），D1/R2 模板源就位，`wrangler deploy` 即可上 CF。
- **遗留（P1）**：WebSocket 真实推送（Durable Objects）、S3/PostgreSQL 后端、D1 建表的
  `start`/`scheduled` 事件接入（P0 用 `wrangler d1 migrations apply` 手动建表）。
