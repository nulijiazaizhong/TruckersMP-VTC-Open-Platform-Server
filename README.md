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
├─ map-dlc/                    # 地图 DLC 版本清单（随仓库自包含，部署不依赖上层）
├─ Dockerfile                  # Docker 多阶段构建（musl 静态 + bundled sqlite）
├─ docker-compose.yml          # docker compose 编排
└─ .env.example
```

**关键点**：`vtc-core` 的 service **泛型 over `Db`/`Storage` 后端**（持有具体 `Arc<D>`，
而非 `Arc<dyn Db>`——`?Send` 的 `dyn` 会反向污染 axum 的 Send 边界）。Docker 注入
SQLite/本地/内存，CF 注入 D1/R2，业务逻辑零重写。

## 构建

### Docker 形态（自部署）

**推荐：直接 `docker compose`**（在项目根目录，Dockerfile / compose / map-dlc 都在仓库内，自包含）：
```bash
# 在项目根目录（仓库根）：
$env:JWT_SECRET = "换一长串随机串"        # PowerShell；Linux/macOS 用 export JWT_SECRET=...
docker compose up -d
curl http://127.0.0.1:8731/api/v1/health   # 返回 {"code":0} 即启动成功
```
> Docker 构建在容器内用 **bundled sqlite**（自带 C 源码现场编译，musl 静态链接），
> **不需要**本机预编译 sqlite，也不需要 `prebuilt/`。`.env` 里的 `JWT_SECRET` 等会自动注入。

<details>
<summary>不用 Docker、本机直接编译二进制（可选）</summary>

```bash
# 本机需 rust 工具链 + 一个 C 编译器（bundled sqlite 要 cc 编译 SQLite C 源码）
# Windows：用 MSVC 或 clang；musl 静态目标需 musl-gcc（见 Dockerfile）
cargo build -p vtc-server --release
# 产物：target/release/vtc-server（Linux musl 静态 / 本机 gnu 动态）
```
</details>

### Cloudflare Workers 形态

**推荐部署方式：在 Cloudflare 控制台「连接 GitHub 仓库」，之后 push `main` 自动构建 + 部署**，
全程网页点选，**不用装 wrangler / Rust / Node**（Rust 工具链由 Cloudflare 构建时自动装）。
最短路径见下方 **[Cloudflare Workers 部署（连 GitHub 仓库）](#cloudflare-workers-部署)**。

> 本地构建（仅调试 / 手动上传时用）：
> ```bash
> cargo build -p vtc-worker --target wasm32-wasip2 --release
> # 产物：target/wasm32-wasip2/release/vtc_worker.wasm（release 约 59 KB，已开 lto/strip）
> ```

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
- **C 依赖**：手写 JWT/PBKDF2（不拉 jsonwebtoken/pbkdf2 → 避免 ring C 编译）；
  SQLite 用 **bundled**（自带 C 源码现场编译）。Docker 里 musl 静态链接必须 bundled——
  系统 `libsqlite3.a` 混入 glibc 启动对象会导致运行时 SIGSEGV（139），bundled 从根上避开；
  且 bundled 让 Windows 本机构建也不再需要预编译 `prebuilt/sqlite3.lib`。CF Worker 形态零 C（用 D1）。

## Cloudflare Workers 部署

> **服务端两套部署**：**① Docker 自部署**（[docker compose](#构建)）；**② Cloudflare Workers**（本节）。
> 本节只讲**一条推荐路径**：在 Cloudflare 控制台**「连接 GitHub 仓库」**，之后 push `main` 自动构建 + 部署。
> 全程网页点选，**不用在本机装 wrangler / Rust / Node**——Rust 工具链由 Cloudflare 在构建时自动安装。
>
> 想要「不连仓库、手动拖 wasm」的一次性部署？见本节末尾 [附：手动上传 wasm](#附手动上传-wasm不连仓库)。

worker 形态**零 C 依赖**（D1/R2 走 CF 托管，无 SQLite、无本地存储）。**一份代码、一套服务逻辑**，
与 Docker 形态完全共享 `vtc-core`。

**首次访问自愈（省心关键）**：worker 在每个请求前自检「D1 建表没有 / admin 有没有」，缺了自动补
（`src/lib.rs` 的 `ensure_ready`：幂等 `db::migrate` 建 13 张表 + seed admin 与默认车队）。
所以**不手动粘贴 SQL、不配 cron、不手动建表**——建好 D1/R2、连上仓库、部署，
**第一次访问即自动建库并种进 admin**（冷启动约 +0.5s，幂等只跑一次）。

---

### 部署主路径：连接 GitHub 仓库（push 即部署）

#### 第 1 步 · 先建好 D1 数据库 + R2 桶（网页端，各 1 分钟）

部署 worker 前，先在 Cloudflare 建好它要用的两个资源（**只建空库/空桶，不跑任何 SQL**）：

1. **建 D1**：控制台左侧 **Workers & Pages → D1 数据库** → **Create a database** → 选区域、起名（如 `vtc`）。
   记住库名 `vtc`（第 3 步绑定 D1 时选它）。
2. **建 R2**：控制台 **Workers & Pages → R2 对象存储** → **Create a bucket** → 命名（如 `vtc-saves`）、选区域。
   记住桶名 `vtc-saves`（第 3 步绑定 R2 时选它）。

> 不需要进 SQL 编辑器建表——worker 首次访问会自动建全部 13 张表（见上「首次访问自愈」）。

#### 第 2 步 · 连接 GitHub 仓库（创建 worker 的入口）

1. 控制台 **Workers & Pages → 创建 Worker（Create a Worker）**。
2. **选「连接 GitHub 仓库 / Connect Git repository」**（不是「创建 Worker」模板、不是「上传代码」）。
3. 首次会要求**授权 Cloudflare 的 GitHub 应用**（点 `Configure Cloudflare` → 允许访问）。
4. 选仓库 **`nulijiazaizhong/TruckersMP-VTC-Open-Platform-Server`**、分支 **`main`**。
5. 进入「**设置应用程序**」页，**按下表逐项填**（CF 默认值按 Node 项目预设，对 Rust 仓库全部要改）：

| 表单字段 | 填什么 | 说明 |
|---|---|---|
| **项目名称** | `vtc-server` | CF 默认填仓库 slug，建议改成和 worker 名一致（非必须）。 |
| **构建命令** | 见下方「构建命令」整段 | CF 镜像无 Rust，**必须**现场装 rustup + wasm target，再跑仓库脚本。 |
| **部署命令** | 见下方「部署命令」整段 | 装 wrangler、把配置拷进产物目录再 `wrangler deploy`。 |
| **预览命令** | `npm i -g wrangler@3 && cd crates/vtc-worker/dist && wrangler preview` | **必填字段**（CF 表单不允许空）。它只用于本地调试，**不影响正式部署**（部署走上面的「部署命令」）。worker-rs 本地预览可能因 `nodejs_compat` 模拟不完美而报错，**报错了也不影响 `wrangler deploy` 上线**，可忽略。 |
| **启用预览构建** | **关** | 不需要 CI 跑预览。 |
| **使用 Cloudflare Access 保护** | **关** | API 服务不用，需要时再开。 |
| **高级设置** | 默认即可 | 展开可设 Rust 版本等，不填也行。 |

**构建命令**（整段粘贴）——装 Rust + wasm target，再跑仓库自带脚本。
⚠ **CF Builds 用 `/bin/sh`（dash）执行，不是 bash**：`source` 是 bash 内建，dash 不认
（会报 `source: not found` 直接崩）。改用 **`.`（点号，POSIX 等价）** 或直接改 `PATH`。
本命令直接改 `PATH` 指向 cargo bin，最稳（脚本会 build 并把 `vtc_worker.wasm` + `index.js`
拷进 `dist/`，两个文件必须都在，否则 CF 当成「静态 Worker」绑不上 D1/R2）：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
  && export PATH="$HOME/.cargo/bin:$PATH" \
  && rustup target add wasm32-wasip2 \
  && bash crates/vtc-worker/cf-build.sh
```

**部署命令**（整段粘贴）——装 wrangler，在产物目录里用 `--remote-config` 部署。
（`cf-build.sh` 已经在 `dist/` 里生成了一份**干净的 `wrangler.jsonc`**：无 `build` 字段、无占位符
routes，`main: index.js` 让 wrangler 把 `index.js` + `vtc_worker.wasm` 一起上传 = 「代码 Worker」。
**不要**再 `cp` 仓库原始 `wrangler.jsonc`（它带 `build` 字段会让 wrangler 重跑构建、带 `<D1_DATABASE_ID>`/
`<ZONE_ID>` 占位符会让 deploy 校验失败——这正是「构建失败、线上还是 Hello World」的根因）。
`--remote-config` 从 Cloudflare 拉回该 worker 已保存的 `DB`/`BUCKET` 绑定、secrets、正式域名路由，
覆盖本地配置，占位符免填。）
```bash
npm i -g wrangler@3 \
  && cd crates/vtc-worker/dist \
  && wrangler deploy --remote-config
```

6. 点右下 **部署**。CF 开始首次构建（装 Rust 约 1–2 分钟 + build + deploy），完成后 worker 上线，
   得到 `https://vtc-server.<你的子域>.workers.dev`。**之后每次 push `main` 自动重构建 + 重部署。**

> ⚠ **占位符**：`wrangler.jsonc` 里有 `<D1_DATABASE_ID>` / `<ZONE_ID>` 两个占位符。
> **最简单**：第 3 步在 Dashboard 把 `DB`/`BUCKET` 绑定配好后，把上面「部署命令」最后的
> `wrangler deploy` 改成 **`wrangler deploy --remote-config`**——wrangler 会从 Cloudflare 拉回该
> worker 已保存的绑定/变量/密钥覆盖本地配置，占位符免填、不报错。（这是推荐做法。）
> 或者把占位符填实（D1 的 Database ID、域名的 Zone ID），但容易填错，不如 `--remote-config`。

#### 第 3 步 · 配绑定、密钥、兼容 flag（网页端 Settings，一次性）

构建部署后，进该 worker → **Settings**，配四样（**只需配一次**，之后每次 push 自动复用）：

1. **Bindings**（资源绑定）——名字必须**精确**全大写：
   - **Add D1 database** → 绑定名 **`DB`** → 选第 1 步建的 `vtc`。
   - **Add R2 bucket** → 绑定名 **`BUCKET`** → 选第 1 步建的 `vtc-saves`。
   > 代码按 `env.d1("DB")` / `env.get_binding("BUCKET")` 取，名字对不上运行时报「binding 未找到」。
2. **Variables and Secrets**：
   - **Variables**（非机密）：`TEAM_NAME`=默认车队、`UPDATE_CHANNEL`=stable、
     `PUBLIC_BASE`=你的正式域名（如 `https://vtc.example.com`）、`AUTH_REQUIRE_TMP`=false、`AUTH_REQUIRE_STEAM`=false。
   - **Secrets**（机密，用 **Add secret** 弹窗填）：
     `JWT_SECRET`（生产 ≥32 字符）、`VTC_STORAGE_SIGN_KEY`（存储中转 HMAC）、
     `VTC_ADMIN_PASSWORD`（admin 初始密码，**建议设随机强口令**；不设用内置 `ChangeMe123!`）。
3. **Compatibility**：
   - **Compatibility Date** 选 `2025-08-01`。
   - **Compatibility Flags**：**必须勾 `nodejs_compat`**（worker-rs 上传 wasm 的硬性要求，不勾启动即报错）。

#### 第 4 步 · 验证（触发首次访问自愈）

- 打开 `https://vtc-server.<子域>.workers.dev/api/v1/health` 应返回 `ok`——**这一下就自动建了 13 张表 + 种 admin**。
- `/api/v1/config/public` 返回车队品牌（无鉴权，此时 team 已 seed）。
- 完整冒烟（登录 / 成员 / 存档 / DLC / 审计）见下方 [验证（部署后）](#验证部署后)。

> **可选**：给 DLC 端点传地图模板到 R2 的 `dlc-templates/` 前缀（`dlc-templates/1.61.ets2.txt` 等，
> 内容取自仓库 `map-dlc/`）。**不传也能部署**，只是 `/dlc/map-template` 返回「模板缺失」，不影响其它 API。
> **admin 登录**：账号 `admin`，密码 = `VTC_ADMIN_PASSWORD`（没配则 `ChangeMe123!`），登录后建议立即改密，
> 或用 `POST /api/v1/seed`（body `{"password":"新口令"}`，需 admin Bearer）重置。

### 附：手动上传 wasm（不连仓库）

> 一次性部署、不想连 GitHub 仓库时用。本地 `cargo build -p vtc-worker --target wasm32-wasip2 --release`
> 出 `vtc_worker.wasm`，在控制台 **创建 Worker → 上传代码**，**`vtc_worker.wasm` 和 `index.js`
> （仓库 [`crates/vtc-worker/index.js`](crates/vtc-worker/index.js)）两个一起传**，再按上面第 3 步配
> 绑定 / 密钥 / `nodejs_compat`。
> ⚠ 只传 `.wasm` 不传 `index.js` 会被 CF 当成「静态 Worker」，报「不能将绑定添加到只有静态资产的 Worker」，
> 绑不上 D1/R2——`index.js` 让 CF 认出这是「代码 Worker」，绑定才解锁。

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
- **Cloudflare Workers 形态**：编译绿（`vtc_worker.wasm` 约 59 KB，已开 lto/strip），P0 全量路由与 Docker 同源
  （共享 vtc-core service）。部署走 **Cloudflare 控制台「连接 GitHub 仓库」**（push main 自动构建部署，
  绑 D1/R2 + 勾 `nodejs_compat`），见上方 [Cloudflare Workers 部署](#cloudflare-workers-部署)。
  **首次访问自愈**：worker 第一次请求自动建 13 张表并 seed admin / 默认车队，免手动建表与 cron。
- **遗留（P1）**：WebSocket 真实推送（Durable Objects）、S3/PostgreSQL 后端。
