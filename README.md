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

```bash
cd crates/vtc-worker
cargo build --target wasm32-wasip2 --release   # 产物供 wrangler 用
wrangler d1 migrations apply DB
wrangler secret put JWT_SECRET
wrangler deploy
```

## 与 .NET 版的契约对齐

- REST 路由 / DTO 字段（camelCase）/ 错误码（1001-1008）/ 状态值 —— **与 VTC.Shared 完全一致**，
  现有 Avalonia 客户端**无需改动**即可对接 Rust 后端。
- 地图 DLC 模板文件（`map-dlc/1.61.ets2.txt` 10 条 / `ats` 20 条）复用。
- 种子：admin / `ChangeMe123!`（`ADMIN_INITIAL_PASSWORD` 覆盖）、默认车队。

## 双目标 async 边界（P1 已解决）

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
  - **P0 交付**：Docker 自部署形态（编译绿 + 运行时全链路冒烟通过：health/config/login/members/dlc=10/401）。
  - **P1 适配**：让 service 对后端**泛型**（`<D: Db>` 而非 `Arc<dyn Db>`），即可让同一份 service
    在 Docker（Send 具体类型）与 worker（`?Send` 的 `Rc<D1/R2>`）下分别成立。worker 的 D1 `db.rs` /
    R2 `storage.rs` 源码已就位（`crates/vtc-worker/src/`），wrangler.jsonc + D1 migrations 已配好，
    只差这个泛型化改造即可上 CF。

## 状态

见交付汇报：Docker 形态已验证；worker 形态为 P1 适配（基础设施已就绪）。
