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
> **② Cloudflare Workers**（本节）。方案 ② 有两种触发方式：
> - **②-A 手动上传**（下文 ①–⑦）：本地 `cargo build` 出 `.wasm`，拖进 Dashboard。**最快上手**，适合一次性部署。
> - **②-B 从 Git 仓库自动部署**（[见文末「从 Git 仓库部署」](#从-git-仓库部署-cloudflare-builds--github-actions)）：
>   连 GitHub 仓库，push 即构建部署。**适合持续迭代 / CI/CD**。
>
> 两者产出同一个 worker（同一个 `vtc_worker.wasm` + `index.js`），只是「在哪构建」不同。

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

### 从 Git 仓库部署（推荐：GitHub Actions 全自动流水线）

> 上面的 ①–⑦ 是「手动上传」（本地 build 出 wasm 拖进去）。如果你要**连 GitHub 仓库、push 即部署**，
> 用这一节。**前置**：已建好 D1/R2（同 ①②）、已在 Cloudflare 配好对应 Worker 的绑定与 secret（同 ④）。
> 仓库直连只替代「③ 上传 wasm」这一步——把「手动拖文件」换成「CI 自动 build 后部署」。
>
> 本流程对照 [Cloudflare 官方 Rust 文档](https://developers.cloudflare.com/workers/languages/rust/)：
> 官方部署 = `npx wrangler deploy`（wrangler 调 build 命令产出 wasm + JS 入口再上传），
> 并靠 `[profile.release] lto/strip/codegen-units` 控制 wasm 体积（本仓库已配，见
> `crates/vtc-worker/Cargo.toml`）。worker-rs 的 wasm 由 `wasm-bindgen` 导出 `fetch` 等事件，
> 需一个 JS 入口加载它——本仓库用 `index.js` 手动复刻了官方 `worker-build` 的「生成 JS 入口」步
> （wasip2 + `nodejs_compat` 的新式 worker 不再需要 `worker-build` 的 webpack 打包）。

**为什么 CF 原生「连接 Git 仓库」对 Rust 不顺手**：Cloudflare Builds 的预置构建镜像是
Node/Python/Java/静态，**没有 Rust 镜像**，要在 Build command 里现场装 rustup（脆弱）。
因此**推荐 GitHub Actions**（`dtolnay/rust-toolchain` 是成熟的 rust 安装方式），
仓库已内置现成流水线 [`.github/workflows/deploy.yml`](.github/workflows/deploy.yml)。

#### 方式 1（推荐）· GitHub Actions 全自动（已入库，push 即部署）

流水线逻辑（见 [`deploy.yml`](.github/workflows/deploy.yml)）：
- **任意 PR / 推送 `crates/**`** → `build` job：`cargo build -p vtc-worker --target wasm32-wasip2 --release`
  作**编译门**（编不过即红，挡在合并前）。
- **push `main`** → `deploy` job：复用/重编 wasm → 把 `index.js` + `vtc_worker.wasm` +
  `wrangler.jsonc` 组装进 `crates/vtc-worker/dist/` → 官方
  [`cloudflare/wrangler-action`](https://github.com/cloudflare/wrangler-action) 跑 `wrangler deploy`
  上传（`main=index.js`，wasm+JS 一起传 = 「代码 Worker」，D1/R2 绑定全部生效）。

**启用只需两步**（流水线已在仓库里，无需再贴 YAML）：
1. GitHub 仓库 **Settings → Secrets and variables → actions** 加两个 secret：
   - `CLOUDFLARE_API_TOKEN`（CF API Token，需 **Workers:Edit** 权限）
   - `CLOUDFLARE_ACCOUNT_ID`（CF Account ID）
2. 处理 `wrangler.jsonc` 里两个占位符 `<D1_DATABASE_ID>` / `<ZONE_ID>`（二选一）：
   - **方案 A · 填实占位符**：`<D1_DATABASE_ID>` ← D1 库的 Database ID（Dashboard → D1 →
     选库 → **Settings** 可见，或 `npx wrangler d1 list`）；`<ZONE_ID>` ← 你的域名 zone id
     （**仅用自定义域名路由才需要**，Dashboard → 域名 → Overview → **API ID**，或
     `npx wrangler whoami --json`）。> 若暂只用 `workers.dev` 子域，建议先把 `routes` 数组删掉
     > （`workers_dev: true` 已够联调），否则 deploy 会因「zone 无权限」报错。
   - **方案 B · `--remote-config` 复用 Dashboard 配置（推荐，免填占位符）**：
     **前提**是已在 Dashboard 对这个 Worker 配好 `DB`/`BUCKET` 绑定（见上文 ④）。
     然后在 GitHub 仓库 **Settings → Variables** 加 `CF_USE_REMOTE_CONFIG = "true"`，
     流水线 deploy 步就自动跑 `wrangler deploy --remote-config`——wrangler 从 Cloudflare
     拉回该 worker 已保存的 bindings/vars/secrets **覆盖**本地 `wrangler.jsonc` 的
     bindings/routes，占位符免填也不报错。
     > 注意：`cloudflare/wrangler-action` **没有 `flags` 输入**，`--remote-config` 写在
     > `command` 里（`deploy --remote-config`），本流水线已处理好。

之后 push `main` 即自动构建 + 部署；开 PR 时自动跑编译门。

#### 方式 2 · Cloudflare Builds（Dashboard 内「连接 Git 仓库」，备选）

> 适合「不想用 GitHub Actions，直接在 Cloudflare 控制台连仓库」的场景。
> 入口：**Dashboard → Workers & Pages → Create Worker → 选「Connect Git repository」**
> （或已建好的 worker → Settings → Build → Connect Git repository）。
> 授权 GitHub 应用后选仓库 `nulijiazaizhong/TruckersMP-VTC-Open-Platform-Server`、分支 `main`，
> 进入「**设置应用程序**」页（= 下图表单）。**逐项按下面填**（CF 默认值是按 Node 项目预设的，
> 对 Rust 仓库全部要改）：

| 表单字段 | 填什么 | 说明 |
|---|---|---|
| **GitHub 仓库** | `nulijiazaizhong/TruckersMP-VTC-Open-Platform-Server` | 顶部已选的仓库（授权时选的） |
| **项目名称** | `vtc-server`（**建议改**） | CF 默认会填仓库 slug（`truckersmp-vtc-open-platform-server`）；建议改成和 worker 名一致，避免混淆。非必须。 |
| **构建命令**（Build command，可选） | 见下方「构建命令」整段 | CF 默认空 / 或 Node 的 `npm build`；**必须填**，因为 CF 镜像无 Rust |
| **部署命令**（Deploy command） | 见下方「部署命令」整段 | CF 默认 `npx wrangler deploy`；**必须改**，要拷 wrangler 配置进产物目录 |
| **预览命令**（Preview command） | **清空** | CF 默认 `npx wrangler preview`；worker-rs 本地预览易报错（nodejs_compat 模拟），**留空** |
| **启用预览构建**（开关） | **关**（off） | 同上，不需要预览 |
| **使用 Cloudflare Access 保护**（开关） | **关**（off） | 这是给 workers.dev 子域加 Cloudflare Access 门；API 服务一般不用。需要时再开 |
| **高级设置**（折叠） | 按需 | 可设「构建环境」（如 Rust 版本）、缓存目录等；默认即可 |
| **部署**（右下蓝按钮） | 点它触发首次构建 | 之后 `main` 分支每次 push 自动重构建 + 部署 |

**构建命令**（整段粘贴到「构建命令」框）——先装 Rust 工具链 + wasm target，再跑仓库脚本：
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
  && source "$HOME/.cargo/env" \
  && rustup target add wasm32-wasip2 \
  && bash crates/vtc-worker/cf-build.sh
```
（`cf-build.sh` 会 `cargo build -p vtc-worker --target wasm32-wasip2 --release`，并把
`vtc_worker.wasm` + `index.js` 拷进 `crates/vtc-worker/dist/`——产物里**必须同时有 `index.js`**，
否则又是「只有静态资产」、绑不上 D1/R2，见上文 ③。）

**部署命令**（整段粘贴到「部署命令」框）——装 wrangler、把配置拷进产物目录、在产物目录里 deploy：
```bash
npm i -g wrangler@3 \
  && cp crates/vtc-worker/wrangler.jsonc crates/vtc-worker/dist/ \
  && cd crates/vtc-worker/dist \
  && wrangler deploy
```
（`wrangler deploy` 读 `wrangler.jsonc` 的 `main: index.js`，把 `index.js` + `vtc_worker.wasm`
一起上传 = 「代码 Worker」，D1/R2 绑定生效。占位符 `<D1_DATABASE_ID>`/`<ZONE_ID>` 的处理
同方式 1：要么填实，要么把 `wrangler deploy` 改成 `wrangler deploy --remote-config`
复用 Dashboard 已配资源。）

> 构建完成后，worker 即上线。绑定（`DB`/`BUCKET`）、secret（`JWT_SECRET` 等）、`nodejs_compat`
> 在 worker 的 **Settings → Bindings / Variables and Secrets / Compatibility** 里配一次即可，
> 之后每次 push 自动重构建并复用这些配置。

> **两种方式二选一**，**不要同时**给同一 worker 配（会互相覆盖部署）。
> `wrangler.jsonc` 的 `<D1_DATABASE_ID>` / `<ZONE_ID>` 占位符：要么**填实**，要么在 deploy 命令里
> 加 `--remote-config`（即 `wrangler deploy --remote-config`）复用 Dashboard 资源——注意它是
> **deploy 的子命令参数**，不是 wrangler-action 的 `flags` 输入（action 无此字段）。
> `JWT_SECRET` / `VTC_STORAGE_SIGN_KEY` / `VTC_ADMIN_PASSWORD` 三个 secret
> 在 worker 的 Settings → Variables and Secrets 里配好（构建/部署不自动带）。

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
