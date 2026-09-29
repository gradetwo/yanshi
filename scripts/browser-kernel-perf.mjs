#!/usr/bin/env node
// 测量**客户端 WASM 内核**的区域渲染成本（与服务端同口径，便于横向对比）。
//
// 为什么必须单独测：客户端预览走的是内核的 render_region_direct，
// 与服务端 HTTP/PNG 路径不同；服务端的 ns/px 不能直接外推到交互体验。
//
// 前置条件：服务端 + 带远程调试的 Chromium（见 scripts/README.md）。
// 用法（URL 必须带 debug=1，否则内核句柄不暴露）：
//   node scripts/browser-kernel-perf.mjs "http://127.0.0.1:8110/?doc=myDoc&token=...&debug=1"

const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: node scripts/browser-kernel-perf.mjs <viewer-url?debug=1>");
  process.exit(2);
}
if (!url.includes("debug=1")) {
  console.error("URL 需要带 debug=1（内核句柄仅在调试模式暴露）");
  process.exit(2);
}

const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
let target = list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const current = id++;
    pending.set(current, resolve);
    ws.send(JSON.stringify({ id: current, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    ?.result?.value;

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
let ready = null;
for (let attempt = 0; attempt < 150; attempt++) {
  ready = await evaluate("!!window.yanshiKernel && window.yanshiStats.kernelHead > 0");
  if (ready) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
if (!ready) {
  console.error("内核未就绪（服务端 /health 的 wasm 是否为 true？URL 是否带 debug=1？）");
  process.exit(1);
}
const result = await evaluate(`(() => {
  const kernel = window.yanshiKernel;
  const sides = [8, 16, 32, 64, 128, 256, 512];
  const out = [];
  for (const side of sides) {
    // 预热一次，再取 5 次最小值（单次计时噪声大，见 implementation-notes 的教训）。
    kernel.render_region_rgba(0, 0, side, side);
    let best = Infinity;
    for (let round = 0; round < 5; round++) {
      const started = performance.now();
      kernel.render_region_rgba(0, 0, side, side);
      best = Math.min(best, performance.now() - started);
    }
    out.push({ side, ms: best, nsPerPixel: (best * 1e6) / (side * side) });
  }
  return out;
})()`);
console.log("客户端 WASM 内核区域渲染（5 次取最小）:");
for (const row of result) {
  console.log(
    `  ${String(row.side).padStart(4)}²: ${row.ms.toFixed(2)}ms｜每像素 ${row.nsPerPixel.toFixed(1)}ns`,
  );
}
ws.close();
