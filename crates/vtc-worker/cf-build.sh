#!/usr/bin/env bash
# =============================================================================
# VTC 服务端 · Cloudflare Workers 构建脚本（供「从 Git 仓库部署」消费）
#
# 产出 Cloudflare 可部署的 worker 产物（ES Module worker）：
#   $OUTPUT/index.js       —— wasm-bindgen（module target）生成的 JS 胶水，**自实例化**
#                              wasm（`new WebAssembly.Instance(wasmModule, ...)` +
#                              `__wbindgen_start()`），`export function fetch`
#   $OUTPUT/index_bg.wasm  —— wasm-bindgen 重打包的 wasm（标准 wasm，00 61 73 6d 01 00 00 00）
#   $OUTPUT/snippets/      —— worker-macros 生成的状态初始化 JS（index.js 会 import 它）
#   $OUTPUT/worker.js      —— CF modules 入口（`import { fetch } from './index.js';
#                              export default { fetch }`），让 wrangler 走 modules 格式
#   $OUTPUT/wrangler.jsonc —— 部署配置（main=worker.js，D1/R2 绑定按名字）
#
# 为什么目标必须是 wasm32-unknown-unknown（**不是** wasm32-wasip2）：
#   worker 0.8.6 的 worker crate 依赖 wasm-bindgen/web-sys/js-sys（纯 wasm-bindgen 栈）。
#   wasip2 目标产出 WASI reactor 格式 wasm（magic 0d 00 01 00），CF 的 WasmModuleObject
#   只认标准 wasm（01 00 00 00）→ 上传报 "expected version 01 00 00 00, found 0d 00 01 00"。
#   wasm32-unknown-unknown 产出标准 wasm + wasm-bindgen 胶水，CF 能跑。
#
# 为什么 wasm-bindgen 用 **--target module**（worker-build 同款 target，**不是** bundler/web）：
#   - module target 的 index.js 用 `import source wasmModule from "./index_bg.wasm"`
#     （esbuild source-phase import），wrangler v4 能正确内联 wasm；且 index.js **末尾
#     自实例化**：`new WebAssembly.Instance(wasmModule, __wbg_get_imports())` +
#     `wasm.__wbindgen_start()`（完整初始化 wasm 导出表），`export function fetch` 直接可用。
#   - **不用 bundler target**：bundler 的 index.js 有 0.2.128 的 re-export bug
#     （`export { __wbg_reset_state } from "./index_bg.js"` 但 index_bg.js 没导出它），
#     且 wrangler 内联 bundler 的 `import * as wasm` 时 `wasm.fetch`/`__wbindgen_start`
#     取不到（CF 反复报 `wasm.fetch is not a function` / `__wbindgen_start is not a function`）。
#   - **不用 web target**：web 用运行时 `__wbg_load` async 加载 wasm，CF worker 无文件系统
#     断链 → `wasm` 永远 undefined。
#   - **不用 worker-build**：worker-build（0.8.x）给 wasm-bindgen 传
#     `--force-enable-abort-handler`（catch wrappers），与本项目 `panic=abort` 冲突
#     （"externref table required for catch wrappers"）。手动跑 wasm-bindgen 时**不传**
#     该 flag 即可绕过；module target 的 index.js 已自实例化，无需 worker-build 的额外处理。
#
# 为什么还要 worker.js 包一层 `export default { fetch }`：
#   module target 的 index.js 是 `export function fetch`（命名导出，**没有** export
#   default）。wrangler 判定 worker 格式时，「有顶层 export 但**没有** export default」
#   会退回 service-worker 格式（认 self.onfetch，不认顶层 fetch 导出）→ 请求进来 0ms
#   空返回（CF 日志 outcome:ok 但 wallTimeMs:0，客户端 522/超时）。worker.js 把它转成
#   CF modules 格式期望的 `export default { fetch }`；`import { fetch } from './index.js'`
#   会触发 index.js 整个模块加载（含末尾的 wasm 实例化 + __wbindgen_start），初始化完成。
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
fi
echo "==> wasm-bindgen: $(wasm-bindgen --version)"

echo "==> cargo build -p vtc-worker --target wasm32-unknown-unknown --release"
cargo build -p vtc-worker --target wasm32-unknown-unknown --release

WASM_TARGET="$REPO_ROOT/target/wasm32-unknown-unknown/release/vtc_worker.wasm"
if [[ ! -f "$WASM_TARGET" ]]; then
  echo "!! 未找到 vtc_worker.wasm（编译可能失败）" >&2
  exit 1
fi

# 清产物目录，跑 wasm-bindgen 生成 JS 胶水 + 重打包 wasm。
# ★ --target module（worker-build 同款；见文件头注释为何不用 bundler/web/worker-build）。
# ★ 不传 --force-enable-abort-handler（panic=abort 不需要 catch wrappers，绕开
#   worker-build 的 "externref table required" 死锁）。
mkdir -p "$OUTPUT"
rm -rf "$OUTPUT"/*
wasm-bindgen "$WASM_TARGET" \
  --out-dir "$OUTPUT" \
  --no-typescript \
  --target module \
  --out-name index

if [[ ! -f "$OUTPUT/index.js" || ! -f "$OUTPUT/index_bg.wasm" ]]; then
  echo "!! wasm-bindgen 未产出 index.js / index_bg.wasm" >&2
  ls -lh "$OUTPUT" >&2
  exit 1
fi

# ★ 生成 CF modules 入口 worker.js（module target 版）。
#   `import { fetch } from './index.js'` 触发 index.js 整个模块加载：
#     - 顶部 `import source wasmModule from "./index_bg.wasm"`（wrangler 内联 wasm）
#     - 末尾 `new WebAssembly.Instance(wasmModule, __wbg_get_imports())` +
#       `wasm.__wbindgen_start()`（自实例化 + 激活导出表）
#   然后 `export default { fetch }` 让 wrangler 走 modules 格式（认 default 对象，
#   否则「有 export 无 default」退回 service-worker 格式 → 0ms 空返回）。
cat > "$OUTPUT/worker.js" <<'JS'
import { fetch } from './index.js';
export default { fetch };
JS

# 生成一份「部署用」的干净 wrangler.jsonc 到产物目录：
#   - 无 build 字段：CF Builds 已单独跑构建命令，wrangler deploy 不该再跑 cf-build.sh。
#   - 无 routes：正式域名路由在 Cloudflare Dashboard 配（wrangler deploy 按名字匹配绑定）。
#   - main = worker.js（modules 包装入口），它 import index.js（自实例化 wasm）。
#   - ★ wrangler v4 语法：D1/R2 用顶层 `d1_databases` / `r2_buckets`（**不是** `bindings`
#     数组——那个字段 v3/v4 都不认，会被忽略且 D1/R2 绑不上）。按 database_name /
#     bucket_name 名字引用（v4 支持，不强制 database_id/bucket_id，CF 按名字找已建的）。
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
echo "==> 完成。CF 入口 = worker.js（import index.js 自实例化 wasm），部署配置 = wrangler.jsonc"
echo "==> 部署命令：npm i -g wrangler@4 && cd <dist> && wrangler deploy"
