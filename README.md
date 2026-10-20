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
│       └─ D1Db(worker::d1) + R2Storage + worker.rs 路由入口
│       └─ wrangler.jsonc（CI/CD 可选）+ migrations/（手动改 schema 参考）
├─ map-dlc/                    # 复用上层模板
├─ docker/                     # Dockerfile + docker-compose
└─ .env.example
```

**关键点**：`vtc-core` 的 service **泛型 over `Db`/`Storage` 后端**（持有具体 `Arc<D>`，
而非 `Arc<dyn Db>`——`?Send` 的 `dyn` 会反向污染 axum 的 Send 边界）。Docker 注入
SQLite/本地/内存，CF 注入 D1/R2，业务逻辑零重写。

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

只需 `cargo build` 出一个 `.wasm`，把它拖进 Cloudflare 控制台即可（**不装 wrangler / 不建 CLI**）。
完整网页端步骤见下方 **[Cloudflare Workers 部署](#cloudflare-workers-部署)**。

```bash
cargo build -p vtc-worker --target wasm32-wasip2 --release
# 产物：target/wasm32-wasip2/release/vtc_worker.wasm（release 约 22 MB）
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

> **服务端只有两套部署方案**：**① Docker 自部署**（[docker compose](#构建)，下面略）；
> **② Cloudflare Workers 网页端 Dashboard**（本节，全程在 <https://dash.cloudflare.com> 点选）。
> 下面就是方案 ② 的全部步骤。

worker 形态**零 C 依赖**（D1/R2 走 CF 托管，无 SQLite、无本地存储），`cargo build` 直接出 wasm，
不需要 Docker 形态那套预编译 SQLite。**一条代码、一套服务逻辑**，与 Docker 完全共享。

**核心简化——首次访问自愈**：worker 在**每个请求前**自检「D1 建表没有 / admin 有没有」，缺了自动补
（`src/lib.rs` 的 `ensure_ready`：跑幂等 `db::migrate` 建 13 张表 + seed admin 与默认车队）。
所以**不手动粘贴 SQL、不配 scheduled cron、不手动建表**：建好 D1/R2、绑名、上传 wasm，
**第一次访问即自动把库建好并种进 admin**（冷启动约 +0.5s，幂等只跑一次）。

**唯一要本地做的**：`cargo build` 出一个 `.wasm`，拖进控制台。

---

### 网页端 Dashboard 部署（方案 ②）

> 全程在 <https://dash.cloudflare.com> 操作，**不用装 wrangler / Node / Rust 以外的工具**。

**前提**：已 `cargo build -p vtc-worker --target wasm32-wasip2 --release`，得到
`target/wasm32-wasip2/release/vtc_worker.wasm`（约 22 MB），下面要上传。

#### ① 建 D1 数据库（网页端，**只建空库，不跑 SQL**）

1. 控制台左侧 **Workers & Pages → D1 数据库**（或顶部搜「D1」）。
2. 点 **Create a database**（创建数据库），选区域，起个名字（如 `vtc`）。
3. 记住这个 D1 库的**名字**（`vtc`）——后面「配置 Worker」绑定 D1 时选它。
   > **不需要**进 SQL 编辑器跑建表语句：worker 首次访问会自动建全部 13 张表（见「首次访问自愈」）。
   > 若你之后要手动改 schema，再进 **SQL** 页追加 `CREATE / ALTER`（都带 `IF NOT EXISTS`，幂等）。

#### ② 建 R2 存储桶（网页端）

1. 控制台左侧 **Workers & Pages → R2 对象存储**（或搜「R2」）。
2. 点 **Create a bucket**，命名（如 `vtc-saves`），选区域。
3. 记下桶名（`vtc-saves`）——后面「配置 Worker」绑定 R2 时选它。
4. （可暂缓）存档与 DLC 模板都放这个桶；先建空桶也行，DLC 模板稍后从「④ 配路由」前的 ③ 步补传。

#### ③ 创建 Worker 并上传 wasm + index.js（网页端，**两个文件都要传**）

1. 控制台左侧 **Workers & Pages → 创建 Worker**（Create Worker）。
2. **选「上传代码 / Upload code」**（不是「粘贴 JS / Create Worker」模板）。
3. **同时上传两个文件**（这一步最容易出错，缺一不可）：
   - `vtc_worker.wasm`（上面构建出的 wasm）
   - `index.js`（仓库 [`crates/vtc-worker/index.js`](crates/vtc-worker/index.js)，**必须和 wasm 一起传**）
   > **为什么必须有 `index.js`**：CF 区分「代码 Worker」（JS + 二进制）与「静态 Worker」。
   > worker-rs 编出来是**纯 wasm**，若只传 `.wasm` 不传 JS，CF 会把它当成**静态 Worker**
   > ——到「添加绑定」时就会报 **「不能将绑定添加到只有静态资产的 Worker」**（这正是你遇到的提示）。
   > `index.js` 就两行（加载 wasm + `export { fetch }`），让 CF 认出这是**代码 Worker**，绑定就解锁了。
4. 命名（如 `vtc-server`），点 **Deploy**。此时 worker 已上线，先得到
   `https://vtc-server.<你的子域>.workers.dev`。

> ⚠ 若你**已经**只传了 wasm、看到了「只有静态资产」的提示：
> 进入该 Worker → **Settings → 代码 / 部署**，重新「上传代码」，把 `index.js` 和 `vtc_worker.wasm`
> **两个一起**再传一次覆盖（或用控制台顶部的「上传新的 Wasm + 编辑 JS」），绑定入口即恢复。

#### ④ 配置 Worker（网页端 Settings）

进入刚建的 worker → **Settings**（设置）页，分四个子页配：

1. **Observability → Logs**：按需开（可选，便于排查）。
2. **Settings → Bindings**（资源绑定）：
   - **Add D1 database** → 绑定名填 **`DB`**，选 ① 建的 D1 库（`vtc`）。
   - **Add R2 bucket** → 绑定名填 **`BUCKET`**，选 ② 建的 R2 桶（`vtc-saves`）。
   > ⚠ 绑定名必须**精确**是 `DB` 和 `BUCKET`（全大写），代码 `src/lib.rs` 里按这两个名字取
   > `env.d1("DB")` / `env.get_binding("BUCKET")`，名字对不上会运行时报「binding 未找到」。
3. **Settings → Variables and Secrets**（环境变量与密钥）：
   - **Variables**（非机密）：
     `TEAM_NAME`=默认车队、`UPDATE_CHANNEL`=stable、`PUBLIC_BASE`=你的正式域名
     （如 `https://vtc.example.com`）、`AUTH_REQUIRE_TMP`=false、`AUTH_REQUIRE_STEAM`=false。
   - **Secrets**（机密，网页端用 **Add secret** 而不是写进 variables）：
     `JWT_SECRET`（**生产必须 ≥32 字符**）、`VTC_STORAGE_SIGN_KEY`（存储中转 HMAC 密钥）、
     `VTC_ADMIN_PASSWORD`（seed 出的 admin 初始密码，**强烈建议设一个随机强口令**；
     不设则用内置默认 `ChangeMe123!`）。
   > 网页端没有 `wrangler secret put`，对应用 **Variables and Secrets → Add secret** 弹窗填值。
4. **Settings → Compatibility**（兼容性）：
   - **Compatibility Date** 选 **`2025-08-01`**（与 `wrangler.jsonc` 一致）。
   - **Compatibility Flags**：**必须勾选 `nodejs_compat`**（Enable Node.js compatibility）。
     > 这是 worker-rs 上传 wasm 的**硬性要求**：wasm 内的 `wasm-bindgen` 胶水依赖 Node.js
     > 兼容层才能跑。不勾这个 flag，worker 会启动即报错。

#### ⑤ 配路由 / 域名（网页端 Routing）

还是这个 worker 的 **Settings → Routing**（或「Rules / 域名」页）：

1. **Workers Subdomain**（`*.workers.dev`）：默认开，用于联调（`vtc-server.<子域>.workers.dev`）。
2. **Custom Domains / Routes**（正式域名）：把你的域（如 `vtc.example.com`）指过来，
   加两条路由 `/api/*` 与 `/storage/*`（worker 同时服务这两类路径，见 `src/lib.rs`）。
   > 若整站都走 worker（含根路径），可把路由设成 `vtc.example.com/*`。

#### ⑥ 上传 DLC 模板到 R2（网页端，**可选，不影响核心 API**）

worker 无文件系统，DLC 地图模板存 **R2 的 `dlc-templates/` 前缀**
（`/dlc/map-template` 与 `/dlc/disable` 从这里读）。**不传也能部署**，只是这两个 DLC 端点返回「模板缺失」：

1. 控制台进入 ② 建的 R2 桶 → **文件 / Files**。
2. **上传**两个文件，确保 R2 里的最终 key 是：
   - `dlc-templates/1.61.ets2.txt`（内容来自上层 `map-dlc/1.61.ets2.txt`，10 条）
   - `dlc-templates/1.61.ats.txt`（内容来自上层 `map-dlc/1.61.ats.txt`，20 条）
   > R2 网页上传会把本地文件名映射成对象 key；把本地文件先**重命名**成
   > `dlc-templates_1.61.ets2.txt` 再上传，或上传后在 R2 里移动到 `dlc-templates/` 前缀下，
   > 确保最终 key 是 `dlc-templates/1.61.ets2.txt`（worker 按这个 key 读）。
3. 加新游戏版本：把对应 `map-dlc/<major>.<game>.txt` 按同样 key 传到 `dlc-templates/`，
   无需改 worker 代码。

#### ⑦ 验证（网页端，**触发首次访问自愈**）

- 打开 `https://vtc-server.<子域>.workers.dev/api/v1/health` 应返回 `ok`。
  **就是这一下**触发了 `ensure_ready`：自动建 13 张表 + 种入 admin / 默认车队（第一次约慢 0.5s）。
- `/api/v1/config/public` 返回车队品牌（无鉴权，此时 team 已被 seed）。
- 控制台左侧 **Workers → vtc-server → Logs** 可看实时请求日志；
  **Workers → Observability** 看 CPU/内存曲线。
- 完整冒烟（登录 / 成员 CRUD / 存档 / DLC / 审计）见下方 [验证](#验证部署后)。

> **admin 登录**：seed 的账号是 `admin`，密码 = 你配的 `VTC_ADMIN_PASSWORD` secret；
> 没配就是内置默认 `ChangeMe123!`。**登录后建议立即改密**，或用管理员自助端点
> `POST /api/v1/seed`（body `{"password":"新密码"}`，需 admin Bearer）随时重置。
> `JWT_SECRET` 忘配也能登录（worker 用内置开发默认值并打警告），但**生产必须配**。

### 验证（部署后）

```bash
B=<你的 workers.dev 域名>
curl $B/api/v1/health                        # ok（第一次会触发建表 + seed，约慢 0.5s）
curl $B/api/v1/config/public                 # 车队品牌（无鉴权）
curl $B/api/v1/dlc/map-template?game=ets2&major=1.61   # R2 模板（10 条；未传则提示缺失）
# 登录拿 token（admin / 你配的 VTC_ADMIN_PASSWORD，未配则 ChangeMe123!）
curl -X POST $B/api/v1/auth/login -H 'content-type: application/json' \
     -d '{"username":"admin","password":"ChangeMe123!"}'
# 带 Bearer 调管理端点（/members /saves /audit /seed …）
# 重置 admin 密码（仅 admin）：
curl -X POST $B/api/v1/seed -H "authorization: Bearer <TOKEN>" \
     -H 'content-type: application/json' -d '{"password":"新强口令"}'
```

### 形态对照（同一份 service，两种后端）

| 关注点 | Docker 形态（`vtc-server`） | CF Workers 形态（`vtc-worker`） |
|---|---|---|
| 数据库 | SQLite（`rusqlite`，本地文件 `vtc.db`） | **D1**（CF 托管，`db::D1Db` 实现 `Db`） |
| 对象存储 | 本地磁盘（`LocalStorage`） | **R2**（`R2Storage`；预签名 URL → worker 中转 HMAC 签名） |
| DLC 模板 | 本地 `map-dlc/*.txt` 文件 | **R2 `dlc-templates/*.txt`**（`parse_template` 解析，可后传） |
| 建表/种子 | 启动时 `bootstrap::seed`（SQLite 自动迁移） | **首次访问自愈** `ensure_ready`（自动建表 + seed，免手动） |
| 推送 | 内存 `InMemPush`（存根） | P1 接 Durable Objects（P0 内存占位） |
| 路由 | Axum（`{id}` 模板） | `match (path, method)` 段解析（无 axum） |
| async | `#[async_trait]`（Send） | `#[async_trait(?Send)]`（wasm 单线程） |
| 部署 | `docker compose up` | **Cloudflare 网页端 Dashboard**（拖 .wasm + 绑 D1/R2 + 勾 `nodejs_compat`） |

## 状态

- **P0 全量 API**：Docker 形态编译绿 + 运行时全链路冒烟通过（health/config/public/login/members CRUD/
  saves 上传凭证+发布+下载凭证+取消+删除/announcements CRUD/events CRUD/dlc map-template+required+
  report+disable+restore/audit 列表+过滤/存储中转 PUT+GET/401 鉴权边界）。
- **Cloudflare Workers 形态**：编译绿（`vtc_worker.wasm` 22 MB），P0 全量路由与 Docker 同源
  （共享 vtc-core service）。部署走 **Cloudflare 网页端 Dashboard**（全程点选：拖入 .wasm +
  绑 D1/R2 + 勾 `nodejs_compat`），见上方 [Cloudflare Workers 部署](#cloudflare-workers-部署)。
  **首次访问自愈**：worker 第一次请求自动建 13 张表并 seed admin / 默认车队，免手动建表与 cron。
- **遗留（P1）**：WebSocket 真实推送（Durable Objects）、S3/PostgreSQL 后端。
