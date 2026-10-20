#!/usr/bin/env bash
# =============================================================================
# VTC 服务端 · Cloudflare Workers 构建脚本（供「从 Git 仓库部署」消费）
#
# 产出 Cloudflare 可部署的 worker 产物（ES Module worker）：
#   $OUTPUT/index.js       —— wasm-bindgen 生成的 JS 胶水（CF 入口，含 `export function fetch`）
#   $OUTPUT/index_bg.wasm  —— wasm-bindgen 重打包的 wasm（标准 wasm，00 61 73 6d 01 00 00 00）
#   $OUTPUT/snippets/      —— worker-macros 生成的状态初始化 JS（index.js 会 import 它）
#   $OUTPUT/wrangler.jsonc —— 部署配置（main=index.js，D1/R2 绑定按名字）
#
# 为什么目标必须是 wasm32-unknown-unknown（**不是** wasm32-wasip2）：
#   worker 0.8.6 的 worker crate 依赖 wasm-bindgen/web-sys/js-sys（纯 wasm-bindgen 栈）。
#   wasip2 目标产出 WASI reactor 格式 wasm（magic 0d 00 01 00），CF 的 WasmModuleObject
#   只认标准 wasm（01 00 00 00）→ 上传报 "expected version 01 00 00 00, found 0d 00 01 00"。
#   wasm32-unknown-unknown 产出标准 wasm + wasm-bindgen 胶水，CF 能跑。
#
# 为什么手动跑 wasm-bindgen 而**不用** worker-build：
#   worker-build 0.8.6 默认给 wasm-bindgen 传 --force-enable-abort-handler（catch wrappers），
#   需要 wasm 带 reference-types/exception-handling，但它在 wasm-bindgen 步骤（先于 wasm-opt
#   的 --enable-exception-handling）就失败 → "externref table required for catch wrappers"。
#   本项目 panic=abort（workspace [profile.release]），**不需要** abort handler 恢复，
#   所以手动跑 wasm-bindgen 时**不传** --force-enable-abort-handler 即可绕过。
#
# 用法：
#   cf-build.sh                 # 输出到默认 dist/
#   OUTPUT=/some/dir cf-build.sh # 输出到指定目录（Cloudflare Builds 用）
#
# 需要的前置（镜像/环境）：
#   - Rust 工具链（stable ≥ 1.77，worker-rs MSRV）+ cargo
#   - `rustup target add wasm32-unknown-unknown`（若镜像未预装）
#   - `cargo install wasm-bindgen-cli --version 0.2.128`（若镜像未预装；须与 worker 的
#     wasm-bindgen 锁版本一致，见 vtc-worker/Cargo.toml 的 wasm-bindgen="=0.2.128"）
#   - 网络可达 crates.io（拉依赖；建议启用 CF build cache 加速）
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUTPUT="${OUTPUT:-$SCRIPT_DIR/dist}"

echo "==> repo root : $REPO_ROOT"
echo "==> output    : $OUTPUT"

cd "$REPO_ROOT"

# 确保 wasm target 就位（rustup 不可用 / 已装则跳过）
if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
fi

# 确保 wasm-bindgen CLI 就位（版本必须 =0.2.128，对齐 worker crate 的锁版本）
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "==> 安装 wasm-bindgen-cli 0.2.128（首次构建需 ~1-2 分钟编译）"
  cargo install wasm-bindgen-cli --version 0.2.128 --locked
else
  echo "==> wasm-bindgen: $(wasm-bindgen --version)"
fi

echo "==> cargo build -p vtc-worker --target wasm32-unknown-unknown --release"
cargo build -p vtc-worker --target wasm32-unknown-unknown --release

WASM_TARGET="$REPO_ROOT/target/wasm32-unknown-unknown/release/vtc_worker.wasm"
if [[ ! -f "$WASM_TARGET" ]]; then
  echo "!! 未找到 vtc_worker.wasm（编译可能失败）" >&2
  exit 1
fi

# 清产物目录，跑 wasm-bindgen 生成 JS 胶水 + 重打包 wasm。
# ★ --target bundler（**不是** web / module）：
#   - web target：`export function fetch` 直接可用，但 wasm 用**运行时** `__wbg_load`
#     async 加载（`new WebAssembly.Instance(wasmModule, ...)`），CF worker 无文件系统、
#     无人传 wasm bytes → `wasm` 变量永远 undefined → `fetch` 调用崩
#     （CF 日志 `Cannot read properties of undefined (reading 'fetch')`，index.js:491）。
#   - module target：`import source wasmModule from "./index_bg.wasm"`（source phase
#     import），wrangler v3/v4 的 esbuild 解析不了（"Expected 'from' but found 'wasmModule'"）。
#   - **bundler target**：`import * as wasm from "./index_bg.wasm"`（静态 ES module import），
#     wrangler 把 wasm 作为 compiled-wasm module **内联**进 worker bundle（~867KB），
#     CF 运行时 wasm 已就位，`__wbg_set_wasm(wasm)` 注入有效。
# ★ 不传 --force-enable-abort-handler（panic=abort 不需要 catch wrappers，
#   绕开 worker-build 0.8.6 的 "externref table required" 死锁）。
mkdir -p "$OUTPUT"
rm -rf "$OUTPUT"/*
wasm-bindgen "$WASM_TARGET" \
  --out-dir "$OUTPUT" \
  --no-typescript \
  --target bundler \
  --out-name index

if [[ ! -f "$OUTPUT/index_bg.js" || ! -f "$OUTPUT/index_bg.wasm" ]]; then
  echo "!! wasm-bindgen 未产出 index_bg.js / index_bg.wasm" >&2
  ls -lh "$OUTPUT" >&2
  exit 1
fi

# ★ 生成 CF modules 入口 worker.js。两个要点：
#   (1) **直接 import index_bg.js，不经过 index.js**：wasm-bindgen 0.2.128 的 bundler
#       target 产出的 index.js 有一行 `export { ..., __wbg_reset_state, ... } from
#       "./index_bg.js"`，但 index_bg.js **没**导出 `__wbg_reset_state`（0.2.128 的
#       re-export 列表 bug）→ wrangler esbuild 报 "No matching export"。index_bg.js
#       自己**确实** `export function fetch` + `export function __wbg_set_wasm`，
#       直接 import 它即可绕开 index.js 的 bug 行。
#   (2) **export default { fetch }**：wrangler 判定 worker 格式时，「有顶层 export 但
#       没有 export default」会退回 service-worker 格式（认 self.onfetch，不认顶层
#       fetch 导出）→ 请求进来 0ms 空返回（CF 日志 outcome:ok 但 wallTimeMs:0）。
#       CF modules 格式要求 `export default { fetch }`。
#   (3) **`import wasm from "./index_bg.wasm"`（默认导入）+ `__wbg_set_wasm(wasm)`**：
#       把 wrangler 内联的 wasm 注入 index_bg.js 的模块级 `wasm` 变量。
#       ★ 用**默认导入** `import wasm`（不是 `import * as wasm`）：wrangler 把 .wasm 作为
#       compiled-wasm module 内联时，**默认导出**是 wasm 的 exports 对象（含 fetch/
#       __wbindgen_start 等），`import * as wasm` 得到的是命名空间包装（{ default: exports }），
#       顶层取不到 `__wbindgen_start`/`fetch`（部署时报 `wasm.__wbindgen_start is not a
#       function`）。
#       ★ **不调** `__wbindgen_start()`：它在 wrangler 内联的 wrapper 下访问不稳定
#       （部署期 `wasm.__wbindgen_start is not a function`）。wasm 的 start section 在
#       实例化时自动执行，fetch 事件基本路径不依赖 JS 侧 __wbindgen_start（它主要管
#       内存向量长度等次要状态）。若 fetch 运行时报缺初始化，CF 日志会指示，再补。
cat > "$OUTPUT/worker.js" <<'JS'
import wasm from "./index_bg.wasm";
import { __wbg_set_wasm, fetch } from "./index_bg.js";
__wbg_set_wasm(wasm);
export default { fetch };
JS

# 生成一份「部署用」的干净 wrangler.jsonc 到产物目录：
#   - 无 build 字段：CF Builds 已单独跑构建命令，wrangler deploy 不该再跑 cf-build.sh。
#   - 无 routes：正式域名路由在 Cloudflare Dashboard 配（wrangler deploy 按名字匹配绑定）。
#   - main = index.js（wasm-bindgen 胶水），它会加载同目录的 index_bg.wasm + snippets/。
#   - ★ wrangler v4 语法：D1/R2 用顶层 `d1_databases` / `r2_buckets`（**不是** `bindings`
#     数组——那个字段 v3/v4 都不认，会被忽略且 D1/R2 绑不上）。按 database_name /
#     bucket_name 名字引用（v4 支持，不强制 database_id/bucket_id，CF 按名字找已建的）。
# ★ main = worker.js（modules 包装入口），**不是** index.js（wasm-bindgen 命名导出）。
#   见上方 worker.js 生成注释：wrangler 需要 export default 才走 modules 格式。
cat > "$OUTPUT/wrangler.jsonc" <<'JSONC'
{
  "name": "vtc-server",
  "main": "worker.js",
  "compatibility_date": "2025-08-01",
  "compatibility_flags": ["nodejs_compat"],
  "workers_dev": true,
  "observability": { "enabled": true },
  "d1_databases": [{ "binding": "DB", "database_name": "vtc" }],
  "r2_buckets": [{ "binding": "BUCKET", "bucket_name": "vtc-saves" }],
  "vars": {
    "TEAM_NAME": "默认车队",
    "UPDATE_CHANNEL": "stable",
    "PUBLIC_BASE": "https://vtc.yean.team",
    "AUTH_REQUIRE_TMP": "false",
    "AUTH_REQUIRE_STEAM": "false"
  }
}
JSONC

echo "==> 产物就绪："
ls -lhR "$OUTPUT"
echo "==> 完成。CF 入口 = index.js，wasm = index_bg.wasm（+ snippets/），部署配置 = wrangler.jsonc"
echo "==> 部署命令：npm i -g wrangler@4 && cd <dist> && wrangler deploy"
