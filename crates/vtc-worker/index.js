/**
 * VTC 服务端 · Cloudflare Workers 入口（worker-rs 标准伴随脚本）
 *
 * ★ 必须和 vtc_worker.wasm **一起**上传到 Cloudflare（见 README「③ 创建 Worker」）。
 *
 * 为什么需要这个 index.js：
 *   Cloudflare 区分两种 Worker ——
 *     - 「代码 Worker」：JS 源码 + （可选）WASM 二进制，支持 D1/R2/路由/日志等全部绑定；
 *     - 「静态 Worker / 只有静态资产」：只上传一个二进制、没有伴随 JS，CF 退回到
 *       「只服务文件」模式，**不能加任何绑定**（会报「不能将绑定添加到只有静态资产的 Worker」）。
 *   worker-rs 编出来的是**纯 .wasm**（`crate-type = ["cdylib"]`，wasm-bindgen 胶水已编译进 wasm），
 *   必须配这个 index.js 告诉 CF「这是代码 Worker」，它才会：
 *     1) 加载并实例化 wasm（`new WebAssembly.Instance` 自动链接 WASI 预定义 import）；
 *     2) 让 wasm 导出的 `fetch` / `scheduled` 事件成为 CF 可路由的入口。
 *   不传这个 index.js，只传 .wasm，就会被当成静态 Worker → 绑定加不上。
 *
 * 部署（网页端 Dashboard）：
 *   ① cargo build -p vtc-worker --target wasm32-wasip2 --release
 *   ② 控制台 → Workers & Pages → 创建 Worker → 选「上传代码 / Upload」
 *   ③ 同时上传两个文件：vtc_worker.wasm + 本 index.js
 *   ④ 命名 + Deploy；再去 Settings 绑 D1(R2) / 变量 / 勾 nodejs_compat。
 */

// 1) 加载 wasm（CF 运行时把同目录/同上传批次里的 vtc_worker.wasm 作为二进制提供）
const wasm = await WebAssembly.instantiate(
  fetch("vtc_worker.wasm", { env: self }).then((r) => r.arrayBuffer()),
);

// 2) 导出入口。worker-rs 的 #[worker::event(fetch)] 宏保证导出 `fetch`（3 参数：
//    raw Request, Env, Context），CF 直接路由到它。`scheduled` 未注册则无此导出。
export { fetch } from wasm.instance;
