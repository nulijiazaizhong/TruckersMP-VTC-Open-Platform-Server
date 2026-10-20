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

echo "==> 产物就绪："
ls -lh "$OUTPUT"
echo "==> 完成。Cloudflare 入口 JS = index.js，伴随 wasm = vtc_worker.wasm"
