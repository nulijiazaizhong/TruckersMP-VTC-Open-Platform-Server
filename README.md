# VTC 后端 · Rust

> 一份核心代码，**双目标编译**成两个后端：
>
> - **Docker 自部署**（Axum + SQLite + 本地存储 + 内存推送）
> - **Cloudflare Workers**（worker-rs + D1 + R2）
>
> 两者共享同一套 `vtc-core` service，业务逻辑零重写，仅后端注入不同。

## 架构：可插拔后端，一份逻辑双编译

```
vtc-rs/
├─ Cargo.toml                  # workspace
├─ crates/
│  ├─ vtc-core/                # ★ 共享核心（两个二进制都链接）
│  │   ├─ DTO + 常量 + 错误码 + Domain（13 实体）
│  │   ├─ backend::{Db, Storage, Push} trait（可插拔后端抽象）
│  │   ├─ auth（JWT HS256 手写 + PBKDF2 手写，无 ring C 依赖，wasm 安全）
│  │   └─ service（Auth/Member/Save/DLC/Content/Rate —— 后端无关）
│  ├─ vtc-server/              # Docker 形态
│  │   └─ SqliteDb(rusqlite) + LocalStorage + InMemPush + Axum 路由
│  └─ vtc-worker/              # Cloudflare 形态
│       ├─ D1Db(worker::d1) + R2Storage + worker.rs 路由入口
│       ├─ cf-build.sh         # 一键构建 wasm + JS 胶水 + 干净 wrangler.jsonc
│       └─ migrations/         # 手动改 schema 参考（worker 首次访问自动建表）
├─ map-dlc/                    # 地图 DLC 版本清单（随仓库自包含）
├─ Dockerfile                  # Docker 多阶段构建（musl 静态 + bundled sqlite）
├─ docker-compose.yml
└─ .env.example
```

**关键点**：`vtc-core` 的 service **泛型 over `Db`/`Storage` 后端**（持有具体 `Arc<D>`，
而非 `Arc<dyn Db>`——`?Send` 的 `dyn` 会反向污染 axum 的 Send 边界）。Docker 注入 SQLite/本地/内存，
CF 注入 D1/R2，业务逻辑零重写。

## 形态对照（同一份 service，两种后端）

| 关注点    | Docker 形态（`vtc-server`）                 | CF Workers 形态（`vtc-worker`）                               |
| --------- | ------------------------------------------- | ------------------------------------------------------------- |
| 数据库    | SQLite（`rusqlite`，本地文件 `vtc.db`）     | **D1**（CF 托管，`db::D1Db` 实现 `Db`）                       |
| 对象存储  | 本地磁盘（`LocalStorage`）                  | **R2**（`R2Storage`；预签名 URL → worker 中转 HMAC 签名）     |
| DLC 模板  | 本地 `map-dlc/*.txt` 文件                   | **R2 `dlc-templates/*.txt`**（`parse_template` 解析，可后传） |
| 建表/种子 | 启动时 `bootstrap::seed`（SQLite 自动迁移） | **首次访问自愈** `ensure_ready`（自动建表 + seed，免手动）    |
| 推送      | 内存 `InMemPush`（存根）                    | P1 接 Durable Objects（P0 内存占位）                          |
| 路由      | Axum（`{id}` 模板）                         | `match (path, method)` 段解析（无 axum）                      |
| async     | `#[async_trait]`（Send）                    | `#[async_trait(?Send)]`（wasm 单线程）                        |
| 部署      | `docker compose up`                         | **Cloudflare「连接 GitHub 仓库」**（push main 自动构建部署）  |

---

# 部署

两条独立路径，按需选其一：

- **[① Docker 自部署](#①-docker-自部署)** —— 自有服务器 / 自管基础设施
- **[② Cloudflare Workers](#②-cloudflare-workers)** —— 无服务器，push 即部署

## ① Docker 自部署

在项目根目录（`Dockerfile` / `docker-compose.yml` / `map-dlc/` 都在仓库内，自包含）：

```bash
# 1. 配置密钥（.env；PowerShell 用 $env:JWT_SECRET="..."，Linux/macOS 用 export）
#    至少设置：JWT_SECRET（≥32 字符随机串）、VTC_STORAGE_SIGN_KEY
#    详见 .env.example

# 2. 启动
docker compose up -d

# 3. 健康检查
curl http://127.0.0.1:8731/api/v1/health   # 返回 ok / {"code":0} 即启动成功
```

> 容器内用 **bundled sqlite**（自带 C 源码现场编译，musl 静态链接），
> **不需要**本机预编译 sqlite。`.env` 里的 `JWT_SECRET` 等自动注入容器。

<details>
<summary>不用 Docker、本机直接编译二进制（可选）</summary>

```bash
# 本机需 rust 工具链 + 一个 C 编译器（bundled sqlite 用 cc 编译 SQLite C 源码）
# Windows 用 MSVC/clang；musl 静态目标需 musl-gcc（见 Dockerfile）
cargo build -p vtc-server --release
# 产物：target/release/vtc-server
```

</details>

## ② Cloudflare Workers

worker 形态**零 C 依赖**（D1/R2 走 CF 托管，无 SQLite、无本地存储），与 Docker 形态完全共享 `vtc-core`。

**推荐部署方式：在 Cloudflare 控制台「连接 GitHub 仓库」，push `main` 自动构建 + 部署**，
全程网页点选，**不用在本机装 wrangler / Rust / Node**（Rust 工具链由 Cloudflare 构建时自动安装）。

> **首次访问自愈（省心关键）**：worker 在每个请求前自检「D1 建表没有 / admin 有没有」，缺了自动补
> （`src/lib.rs` 的 `ensure_ready`：幂等建 13 张表 + seed admin 与默认车队）。
> 所以**不手动粘贴 SQL、不配 cron、不手动建表**——建好 D1/R2、连上仓库、部署，
> 第一次访问即自动建库并种进 admin（冷启动约 +0.5s，幂等只跑一次）。

### 第 1 步 · 建 D1 数据库 + R2 桶（网页端，各约 1 分钟）

部署前先在 Cloudflare 建好它要用的两个资源（**只建空库/空桶，不跑任何 SQL**）：

1. **建 D1**：控制台 **Workers & Pages → D1 数据库 → Create a database** → 选区域、起名（如 `vtc`）。
2. **建 R2**：控制台 **Workers & Pages → R2 对象存储 → Create a bucket** → 命名（如 `vtc-saves`）、选区域。

> 不需要进 SQL 编辑器建表——worker 首次访问会自动建全部 13 张表。

### 第 2 步 · 连接 GitHub 仓库（创建 worker 的入口）

1. 控制台 **Workers & Pages → 创建 Worker（Create a Worker）**。
2. **选「连接 GitHub 仓库 / Connect Git repository」**（不是模板、不是「上传代码」）。
3. 首次授权 **Cloudflare 的 GitHub 应用**（点 `Configure Cloudflare` → 允许）。
4. 选仓库 **`nulijiazaizhong/TruckersMP-VTC-Open-Platform-Server`**、分支 **`main`**。
5. 在「设置应用程序」页按下表填（CF 默认值按 Node 项目预设，对 Rust 仓库全部要改）：

| 表单字段                        | 填什么                                                                 | 说明                                                                                                                                                                  |
| ------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **项目名称**                    | `vtc-server`                                                           | 建议与 worker 名一致（非必须）。                                                                                                                                      |
| **构建命令**                    | 见下方「构建命令」整段                                                 | CF 镜像无 Rust，**必须**现场装 rustup + wasm target，再跑仓库自带 `cf-build.sh`。                                                                                     |
| **部署命令**                    | 见下方「部署命令」整段                                                 | 装 wrangler、在产物目录 `dist/` 里 `wrangler deploy`。                                                                                                                |
| **预览命令**                    | `npm i -g wrangler@4 && cd crates/vtc-worker/dist && wrangler preview` | 必填（CF 表单不允许空）。仅本地调试用，**不影响正式部署**。worker-rs 本地预览可能因 `nodejs_compat` 模拟不完美报错，**报错也不影响 `wrangler deploy` 上线**，可忽略。 |
| **启用预览构建**                | **关**                                                                 | 不需要 CI 跑预览。                                                                                                                                                    |
| **使用 Cloudflare Access 保护** | **关**                                                                 | API 服务不用，需要时再开。                                                                                                                                            |
| **高级设置**                    | 默认即可                                                               | 不填也行。                                                                                                                                                            |

**构建命令**（整段粘贴）——装 Rust + wasm target，再跑仓库自带脚本。
⚠ **CF Builds 用 `/bin/sh`（dash）执行，不是 bash**：`source` 是 bash 内建，dash 不认
（会报 `source: not found` 直接崩）。本命令直接改 `PATH` 指向 cargo bin，最稳。
`cf-build.sh` 会 build 并把产物（`worker.js` + `index.js` + `index_bg.wasm` + 干净 `wrangler.jsonc`）
拷进 `dist/`——两个入口文件必须都在，否则 CF 当成「静态 Worker」绑不上 D1/R2：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
  && export PATH="$HOME/.cargo/bin:$PATH" \
  && rustup target add wasm32-unknown-unknown \
  && bash crates/vtc-worker/cf-build.sh
```

**部署命令**（整段粘贴）——装 wrangler，在 `cf-build.sh` 产出的 `dist/` 目录里部署。
`cf-build.sh` 已在 `dist/` 生成一份**干净的 `wrangler.jsonc`**：无 `build` 字段、无占位符 routes，
`main: worker.js`，D1/R2 按 `database_name`/`bucket_name` 名字引用（wrangler v4 语法，
`CF_USE_REMOTE_CONFIG` 场景下再用 `--remote-config` 覆盖）。**不要**再 `cp` 仓库根目录那份
`wrangler.jsonc`（它带 `build` 字段和 `<D1_DATABASE_ID>`/`<ZONE_ID>` 占位符，deploy 会失败）：

```bash
npm i -g wrangler@4 \
  && cd crates/vtc-worker/dist \
  && wrangler deploy
```

6. 点 **部署**。CF 首次构建（装 Rust 约 1–2 分钟 + build + deploy），完成后得到
   `https://vtc-server.<你的子域>.workers.dev`。**之后每次 push `main` 自动重构建 + 重部署。**

### 第 3 步 · 配绑定、密钥、兼容 flag（网页端 Settings，一次性）

构建部署后，进该 worker → **Settings**，配四样（**只需配一次**，之后每次 push 自动复用）：

1. **Bindings**（资源绑定）——名字必须**精确**全大写：
   - **Add D1 database** → 绑定名 **`DB`** → 选第 1 步建的 `vtc`。
   - **Add R2 bucket** → 绑定名 **`BUCKET`** → 选第 1 步建的 `vtc-saves`。
     > 代码按 `env.d1("DB")` / `env.get_binding("BUCKET")` 取，名字对不上运行时报「binding 未找到」。
2. **Variables and Secrets**：
   - **Variables**（非机密）：`TEAM_NAME`=默认车队、`UPDATE_CHANNEL`=stable、
     `PUBLIC_BASE`=你的正式域名（如 `https://vtc.yean.team`）、`AUTH_REQUIRE_TMP`=false、`AUTH_REQUIRE_STEAM`=false。
   - **Secrets**（机密，用 **Add secret** 弹窗填）：
     `JWT_SECRET`（生产 ≥32 字符）、`VTC_STORAGE_SIGN_KEY`（存储中转 HMAC）、
     `VTC_ADMIN_PASSWORD`（admin 初始密码，**建议设随机强口令**；不设用内置 `ChangeMe123!`）。
3. **Compatibility**：
   - **Compatibility Date** 选 `2025-08-01`。
   - **Compatibility Flags**：**必须勾 `nodejs_compat`**（worker-rs 上传 wasm 的硬性要求，不勾启动即报错）。

### 第 4 步 · 验证（触发首次访问自愈）

- 打开 `https://vtc-server.<子域>.workers.dev/api/v1/health` 应返回 `ok`——**这一下就自动建 13 张表 + 种 admin**。
- `/api/v1/config/public` 返回车队品牌（无鉴权，此时 team 已 seed）。
- 完整冒烟见下方[验证（部署后）](#验证部署后)。

> **可选**：给 DLC 端点传地图模板到 R2 的 `dlc-templates/` 前缀（内容取自仓库 `map-dlc/`）。
> **不传也能部署**，只是 `/dlc/map-template` 返回「模板缺失」，不影响其它 API。
> **admin 登录**：账号 `admin`，密码 = `VTC_ADMIN_PASSWORD`（没配则 `ChangeMe123!`），登录后建议立即改密，
> 或用 `POST /api/v1/seed`（body `{"password":"新口令"}`，需 admin Bearer）重置。

### 附：手动上传 wasm（不连仓库）

> 一次性部署、不想连 GitHub 仓库时用。本地跑 `bash crates/vtc-worker/cf-build.sh`
> （= `cargo build -p vtc-worker --target wasm32-unknown-unknown --release` + `wasm-bindgen --target module`），
> 在控制台 **创建 Worker → 上传代码**，把 `dist/` 里的 **`worker.js` + `index.js` + `index_bg.wasm`**
> 一起传（`worker.js` 是 `export default { fetch }` 入口，`index_bg.wasm` 是 wasm-bindgen 重打包的 wasm），
> 再按上面第 3 步配绑定 / 密钥 / `nodejs_compat`。
> ⚠ 只传 `.wasm` 不传 `index.js` 会被 CF 当成「静态 Worker」，报「不能将绑定添加到只有静态资产的 Worker」，
> 绑不上 D1/R2——`index.js` 让 CF 认出这是「代码 Worker」，绑定才解锁。

### 验证（部署后）

```bash
B=<你的 workers.dev 域名>
curl $B/api/v1/health                        # ok（第一次会触发建表 + seed，约慢 0.5s）
curl $B/api/v1/config/public                 # 车队品牌（无鉴权）
curl $B/api/v1/dlc/map-template?game=ets2&major=1.61   # R2 模板（未传则提示缺失）
# 登录拿 token（admin / 你配的 VTC_ADMIN_PASSWORD，未配则 ChangeMe123!）
curl -X POST $B/api/v1/auth/login -H 'content-type: application/json' \
     -d '{"username":"admin","password":"ChangeMe123!"}'
# 带 Bearer 调管理端点（/members /saves /audit /seed …）
# 重置 admin 密码（仅 admin）：
curl -X POST $B/api/v1/seed -H "authorization: Bearer <TOKEN>" \
     -H 'content-type: application/json' -d '{"password":"新强口令"}'
```

---

# 架构说明

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
- **C 依赖**：手写 JWT/PBKDF2（不拉 jsonwebtoken/pbkdf2 → 避免 ring C 编译，wasm 安全）；
  SQLite 用 **bundled**（自带 C 源码现场编译）。Docker 里 musl 静态链接必须 bundled——
  系统 `libsqlite3.a` 混入 glibc 启动对象会导致运行时 SIGSEGV（139），bundled 从根上避开；
  且 bundled 让 Windows 本机构建也不再需要预编译 `prebuilt/sqlite3.lib`。CF Worker 形态零 C（用 D1）。

## 关键构建约束（wasm 形态必读）

- **目标必须是 `wasm32-unknown-unknown`**（worker-rs 0.8.6 的 wasm-bindgen web 栈），
  **不是** `wasm32-wasip2`（后者产出 WASI reactor 格式 wasm，CF 只认标准 wasm）。wrangler 用 **v4**。
- **时钟依赖（login 曾 1101 的根因，务必保留）**：workspace `chrono` 必须带 **`wasmbind`** 特性，
  且 `rate.rs` 只能走 `chrono::Utc::now()`，**不能**碰 `std::time::{Instant, SystemTime}`
  ——后者在 `wasm32-unknown-unknown` 上直接 panic「time not implemented on this platform」
  → CF `__wbg_new` Critical（"Worker's code had hung" / 1101）。Docker 侧不受影响（`wasmbind`
  内部 `#[cfg(wasm32)]` 门控）。
- **wasm-bindgen 用 `--target module`**（worker-build 同款），**不传** `--force-enable-abort-handler`
  （`panic=abort` 不需要 catch wrappers）。详见 `crates/vtc-worker/cf-build.sh` 文件头注释。

## 对外 API 契约

- REST 路由 / DTO 字段（camelCase）/ 错误码（1001–1008）/ 状态值保持稳定，
  现有客户端无需改动即可对接本后端。
- 地图 DLC 模板文件（`map-dlc/1.61.ets2.txt` / `1.61.ats.txt`）随仓库自包含。
- 种子：admin / `ChangeMe123!`（`VTC_ADMIN_PASSWORD` 或 `ADMIN_INITIAL_PASSWORD` 覆盖）、默认车队。

## 状态

- **P0 全量 API**：Docker 形态编译绿 + 运行时全链路冒烟通过（health / config/public / login /
  members CRUD / saves 上传凭证+发布+下载凭证+取消+删除 / announcements CRUD / events CRUD /
  dlc map-template+required+report+disable+restore / audit 列表+过滤 / 存储中转 PUT+GET / 401 鉴权边界）。
- **Cloudflare Workers 形态**：编译绿（`vtc_worker.wasm` 约 59 KB，已开 lto/strip），P0 全量路由与
  Docker 同源（共享 vtc-core service）。部署走 **Cloudflare「连接 GitHub 仓库」**（push main 自动构建部署，
  绑 D1/R2 + 勾 `nodejs_compat`），见上方 [Cloudflare Workers](#②-cloudflare-workers)。
  **首次访问自愈**：worker 第一次请求自动建 13 张表并 seed admin / 默认车队，免手动建表与 cron。
  **线上实测**：`/auth/login` 200（access+refresh 双 token）、`/audit` 带 Bearer 200 / 无 Bearer 401，全绿。
- **遗留（P1）**：WebSocket 真实推送（Durable Objects）、S3/PostgreSQL 后端。
- **待清理**：`.github/workflows/deploy.yml` 仍是 `wasm32-wasip2` 的旧流水线，与 `cf-build.sh`
  （`wasm32-unknown-unknown`）不一致；当前部署走 Cloudflare「连接 GitHub 仓库」而非该 workflow，
  启用该 workflow 前需同步目标与 `cf-build.sh` 产物约定。
