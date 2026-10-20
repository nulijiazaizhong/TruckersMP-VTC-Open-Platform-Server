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

两种部署方式，**选其一**：

- **[方式 A · 网页端 Dashboard 部署](#方式-a--网页端-dashboard-部署)** —— 全程在 Cloudflare 控制台点选，
  不装 CLI，适合先快速上线 / 无终端环境。
- **[方式 B · Wrangler CLI 部署](#方式-b--wrangler-cli-部署)** —— 本地建资源 + 部署，适合 CI/CD 与批量运维。

两者产出的 worker 完全一致；只是「在哪配」不同。

---

### 方式 A · 网页端 Dashboard 部署

> 全程在 <https://dash.cloudflare.com> 操作，**不用装 wrangler / Node**。
> 唯一需要本地做的，是 `cargo build` 出一个 `.wasm` 文件，然后把它拖进控制台。

**前提**：已 `cargo build -p vtc-worker --target wasm32-wasip2 --release`，得到
`target/wasm32-wasip2/release/vtc_worker.wasm`（约 22 MB）。把它改名或记住路径，下面要上传。

#### A1. 建 D1 数据库（网页端）

1. 控制台左侧 **Workers & Pages → D1 数据库**（或顶部搜「D1」）。
2. 点 **Create a database**（创建数据库），选区域，起个名字（如 `vtc`）。
3. 创建后进入该库，左侧 **SQL**（或 **Schema**）页签 —— 把
   [`crates/vtc-worker/migrations/0001_init.sql`](crates/vtc-worker/migrations/0001_init.sql)
   的**全部内容**粘贴进 SQL 编辑器，点 **Save / Run**，13 张表 + 索引即建成。
   > 网页端没有「应用 migrations」按钮，所以建表走 SQL 编辑器手动跑一次。
   > 之后每次改 schema，都在这个 SQL 页追加 `CREATE TABLE / ALTER` 语句重跑即可（语句都带 `IF NOT EXISTS`，幂等）。
4. 记住这个 D1 库的**名字**（`vtc`）——A4 绑定 D1 时选它。

#### A2. 建 R2 存储桶（网页端）

1. 控制台左侧 **Workers & Pages → R2 对象存储**（或搜「R2」）。
2. 点 **Create a bucket**，命名（如 `vtc-saves`），选区域。
3. 记下桶名（`vtc-saves`）——A4 绑定 R2 时选它。
4. （可暂缓）DLC 模板稍后从 A6 上传，先建空桶也行。

#### A3. 创建 Worker 并上传 wasm（网页端）

1. 控制台左侧 **Workers & Pages → 创建 Worker**（Create Worker）。
2. **关键：选「上传代码 / Upload」而不是「创建 Worker（粘贴 JS）」**——
   直接上传一个 `.wasm` 文件（Cloudflare 支持「上传 Wasm 文件」模式）。
3. 选 **上传 Wasm 文件**（Upload Wasm），把上面构建出的 `vtc_worker.wasm` 拖进去。
   > 选「**作为 Worker 直接运行**」路径（不是「JS 里 `instantiate`」）——
   > worker-rs 的 wasm 自带 `wasm-bindgen` 胶水，由 CF 运行时直接加载，**不需要**再写 JS 入口。
4. 命名（如 `vtc-server`），点 **Deploy**。此时 worker 已上线，先得到
   `https://vtc-server.<你的子域>.workers.dev`。

#### A4. 配置 Worker（网页端 Settings）

进入刚建的 worker → **Settings**（设置）页，分四个子页配：

1. **Observability → Logs**：按需开（可选，便于排查）。
2. **Settings → Bindings**（资源绑定）：
   - **Add D1 database** → 绑定名填 **`DB`**，选 A1 建的 D1 库（`vtc`）。
   - **Add R2 bucket** → 绑定名填 **`BUCKET`**，选 A2 建的 R2 桶（`vtc-saves`）。
   - （可选）**Durable Object**：绑定名 `PUSH`、class `PushRoom` —— P0 用内存存根，
     不绑定也能跑；P1 接 WebSocket 推送时再加。
   > ⚠ 绑定名必须**精确**是 `DB` 和 `BUCKET`（全大写），代码 `src/lib.rs` 里按这两个名字取
   > `env.d1("DB")` / `env.get_binding("BUCKET")`，名字对不上会运行时报「binding 未找到」。
3. **Settings → Variables and Secrets**（环境变量与密钥）：
   - **Variables**（非机密）：
     `TEAM_NAME`=默认车队、`UPDATE_CHANNEL`=stable、`PUBLIC_BASE`=你的正式域名
     （如 `https://vtc.example.com`）、`AUTH_REQUIRE_TMP`=false、`AUTH_REQUIRE_STEAM`=false。
   - **Secrets**（机密，网页端用 **Add secret** 而不是写进 variables）：
     `JWT_SECRET`（**生产必须 ≥32 字符**）、`VTC_STORAGE_SIGN_KEY`（存储中转 HMAC 密钥）。
   > 网页端没有 `wrangler secret put`，对应用 **Variables and Secrets → Add secret** 弹窗填值。
4. **Settings → Compatibility**（兼容性）：
   - **Compatibility Date** 选 **`2025-08-01`**（与 `wrangler.jsonc` 一致）。
   - **Compatibility Flags**：**必须勾选 `nodejs_compat`**（Enable Node.js compatibility）。
     > 这是 worker-rs 上传 wasm 的**硬性要求**：wasm 内的 `wasm-bindgen` 胶水依赖 Node.js
     > 兼容层才能跑。不勾这个 flag，worker 会启动即报错。

#### A5. 配路由 / 域名（网页端 Routing）

还是这个 worker 的 **Settings → Routing**（或「Rules / 域名」页）：

1. **Workers Subdomain**（`*.workers.dev`）：默认开，用于联调（`vtc-server.<子域>.workers.dev`）。
2. **Custom Domains / Routes**（正式域名）：把你的域（如 `vtc.example.com`）指过来，
   加两条路由 `/api/*` 与 `/storage/*`（worker 同时服务这两类路径，见 `src/lib.rs`）。
   > 若整站都走 worker（含根路径），可把路由设成 `vtc.example.com/*`。

#### A6. 上传 DLC 模板到 R2（网页端）

worker 无文件系统，DLC 地图模板改存 **R2 的 `dlc-templates/` 前缀**
（`/dlc/map-template` 与 `/dlc/disable` 从这里读）：

1. 控制台进入 A2 建的 R2 桶 → **文件 / Files**。
2. **上传**两个文件，并把 R2 里的「路径/前缀」写成 `dlc-templates/`：
   - `dlc-templates/1.61.ets2.txt`（内容来自上层 `map-dlc/1.61.ets2.txt`，10 条）
   - `dlc-templates/1.61.ats.txt`（内容来自上层 `map-dlc/1.61.ats.txt`，20 条）
   > R2 网页上传会把本地文件名映射成对象 key；把本地文件先**重命名**成
   > `dlc-templates_1.61.ets2.txt` 再上传，或上传后在 R2 里移动到 `dlc-templates/` 前缀下，
   > 确保最终 key 是 `dlc-templates/1.61.ets2.txt`（worker 按这个 key 读）。
3. 加新游戏版本：把对应 `map-dlc/<major>.<game>.txt` 按同样 key 传到 `dlc-templates/`，
   无需改 worker 代码。

#### A7. 验证（网页端）

- 打开 `https://vtc-server.<子域>.workers.dev/api/v1/health` 应返回 `ok`。
- `/api/v1/config/public` 返回车队品牌（无鉴权）。
- 控制台左侧 **Workers → vtc-server → Logs** 可看实时请求日志；
  **Workers → Observability** 看 CPU/内存曲线。
- 完整冒烟（登录 / 成员 CRUD / 存档 / DLC / 审计）见下方 [验证](#验证部署后)。

> **首次部署后 admin 登录**：seed 的 admin 账号密码是 `admin` / `ChangeMe123!`，
> 在 D1 里改（或 P1 加管理界面后改）。`JWT_SECRET` 忘配也能登录（worker 用内置开发默认值并打警告），
> 但**生产必须配**。

---

### 方式 B · Wrangler CLI 部署

> 本地建资源 + 部署，适合 CI/CD、批量运维、或想把配置写进 `wrangler.jsonc` 版本化的场景。
> `crates/vtc-worker/wrangler.jsonc` 已写好 binding/vars/路由模板，下面主要把占位符填实。

#### B1. 前置：本地资源（一次性）

在 `crates/vtc-worker/` 下执行（需 `wrangler`，`npm i -g wrangler` 或 `npm i -g wrangler@3`）：

```bash
cd crates/vtc-worker

# 1) 创建 D1（= SQLite 方言的关系型数据库；返回 database_id）
wrangler d1 create vtc
#    → 把返回的 database_id 填进 wrangler.jsonc 的 bindings[DB].database_id

# 2) 创建 R2 桶（对象存储：存档 + DLC 模板都放这）
wrangler r2 bucket create vtc-saves

# 3) 应用 D1 建表迁移（13 张表；等价于网页端的 A1「SQL 编辑器粘贴 0001_init.sql」）
wrangler d1 migrations apply DB
```

#### B2. 填 wrangler.jsonc

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

> `wrangler.jsonc` 的 `compatibility_flags: ["nodejs_compat"]` 与 `compatibility_date` 已配好，
> 对应网页端的 A4「Compatibility」，CLI 下不用手动勾。

#### B3. 上传 DLC 模板到 R2

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

#### B4. 构建 + 部署

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
  （共享 vtc-core service），D1/R2 模板源就位。部署两条路：**网页端 Dashboard**（全程点选，
  拖入 .wasm + 绑 D1/R2 + 勾 `nodejs_compat`）或 **Wrangler CLI**（`wrangler deploy`），见上方
  [Cloudflare Workers 部署](#cloudflare-workers-部署)。
- **遗留（P1）**：WebSocket 真实推送（Durable Objects）、S3/PostgreSQL 后端、D1 建表的
  `start`/`scheduled` 事件接入（P0 网页端在 D1 的 SQL 编辑器手动跑 `0001_init.sql`，
  CLI 用 `wrangler d1 migrations apply`）。
