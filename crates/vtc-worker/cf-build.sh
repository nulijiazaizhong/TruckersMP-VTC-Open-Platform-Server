#!/usr/bin/env bash
# =============================================================================
# VTC 服务端 · Cloudflare Workers 构建脚本（供「从 Git 仓库部署」消费）
#
# 产出 Cloudflare 可部署的 worker 产物：
#   $OUTPUT/vtc_worker.wasm   —— cargo 编译出的 wasm（worker-rs cdylib）
#   $OUTPUT/index.js          —— 伴随 JS（让 CF 识别为「代码 Worker」，可加 D1/R2 绑定）
#
# 用法：
#   cf-build.sh                 # 输出到默认 dist/
#   OUTPUT=/some/dir cf-build.sh # 输出到指定目录（Cloudflare Builds 用）
#
# 需要的前置（镜像/环境）：
#   - Rust 工具链（stable ≥ 1.85）+ cargo
#   - `rustup target add wasm32-wasip2`（若镜像未预装）
#   - 网络可达 crates.io（拉依赖；建议启用 CF build cache 加速）
#
# 说明：本仓库用 workspace，worker 是 `vtc-worker` crate。脚本从仓库根
# （本文件的上一级目录）运行 `cargo build -p vtc-worker --target wasm32-wasip2 --release`。
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
  rustup target add wasm32-wasip2 >/dev/null 2>&1 || true
fi

echo "==> cargo build -p vtc-worker --target wasm32-wasip2 --release"
cargo build -p vtc-worker --target wasm32-wasip2 --release

WASM_TARGET="$(ls target/*/release/vtc_worker.wasm target/wasm32-wasip2/release/vtc_worker.wasm 2>/dev/null | head -n1)"
if [[ -z "$WASM_TARGET" || ! -f "$WASM_TARGET" ]]; then
  echo "!! 未找到 vtc_worker.wasm（编译可能失败）" >&2
  exit 1
fi

mkdir -p "$OUTPUT"
cp "$WASM_TARGET" "$OUTPUT/vtc_worker.wasm"
cp "$SCRIPT_DIR/index.js" "$OUTPUT/index.js"

# 生成一份「部署用」的干净 wrangler.jsonc 到产物目录（覆盖原始那份）：
#   - 去掉 build 字段：CF Builds 已经单独跑过构建命令产出 wasm，wrangler deploy
#     不该再跑 `bash cf-build.sh`（在 dist/ 里没有 cargo/crate，会失败 → 整个构建红）。
#   - 去掉 routes：本地 <ZONE_ID> 占位符会让 wrangler deploy 校验失败；
#     正式域名路由改用 --remote-config 从 Cloudflare 拉回（Dashboard 已配的）。
#   - bindings 保留名字（DB/BUCKET），database_id/zone_id 的占位符由
#     `wrangler deploy --remote-config` 用 Cloudflare 上已保存的值覆盖。
#   - 保留 name / main / compatibility_date / compatibility_flags / vars。
cat > "$OUTPUT/wrangler.jsonc" <<'JSONC'
{
  "name": "vtc-server",
  "main": "index.js",
  "compatibility_date": "2025-08-01",
  "compatibility_flags": ["nodejs_compat"],
  "workers_dev": true,
  "observability": { "enabled": true },
  "bindings": [
    { "binding": "DB", "type": "d1", "database_name": "vtc" },
    { "binding": "BUCKET", "type": "r2-bucket", "bucket_name": "vtc-saves" }
  ],
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
ls -lh "$OUTPUT"
echo "==> 完成。Cloudflare 入口 JS = index.js，伴随 wasm = vtc_worker.wasm，部署配置 = wrangler.jsonc"
echo "==> 部署命令请用：wrangler deploy --remote-config（拉回 Cloudflare 已配的绑定/密钥/路由）"
