#!/usr/bin/env node
// 测量**客户端 WASM 内核**的区域渲染成本（与服务端同口径，便于横向对比）。
//
// 为什么必须单独测：客户端预览走的是内核的 render_region_direct，
// 与服务端 HTTP/PNG 路径不同；服务端的 ns/px 不能直接外推到交互体验。
//
// 前置条件：服务端 + 带远程调试的 Chromium（见 scripts/README.md）。
// 用法（URL 必须带 debug=1，否则内核句柄不暴露）：
//   node scripts/browser-kernel-perf.mjs "http://127.0.0.1:8110/?doc=myDoc&token=...&debug=1"

// **需要 `debug=1` 就自己带上** ✓（第 413 轮 ✓）：runner 的注释写明"需要它的那一条自己传" ✓
// —— 因为给**全体**浏览器判据加 `debug=1` 会改变页面的 SW/离线行为 ✗（第 338 轮实测：
// 9 条本来绿的判据开始报 `undici:15270` ✗）。原先这里只是**报错退出** ✗ ⇒ 这条判据在 CI 里
// **永远红** ✗（`EXIT=2` ✓，而"意外失败"还被算成 0 ✗ —— 第 412 轮才修好那个计数 ✓）。
const rawUrl = process.argv[2];
const url = rawUrl && !rawUrl.includes("debug=1")
  ? `${rawUrl}${rawUrl.includes("?") ? "&" : "?"}debug=1`
  : rawUrl;
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: node scripts/browser-kernel-perf.mjs <viewer-url?debug=1>");
  process.exit(2);
}
// （原先此处是"缺 `debug=1` 就报错退出" ✗ ⇒ 那条路现在不会再走到 ✓ —— 上面已按需补上 ✓。）

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
const evaluate = async (expression) => {
  const response = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (response.result?.exceptionDetails) {
    console.error("页面内求值异常:", JSON.stringify(response.result.exceptionDetails).slice(0, 400));
  }
  return response.result?.result?.value;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
let ready = null;
for (let attempt = 0; attempt < 150; attempt++) {
  // 就绪判定：内核句柄存在且 head 已加载（空文档 head 为 0，因此不能用 > 0 判定）。
  ready = await evaluate(
    "!!window.yanshiKernel && window.yanshiStats.wasm === true && window.yanshiStats.kernelHead !== null",
  );
  if (ready) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
if (!ready) {
  console.error("内核未就绪（服务端 /health 的 wasm 是否为 true？URL 是否带 debug=1？）");
  process.exit(1);
}
// 二分：固定开销落在哪一段（JS↔WASM 调用 / 渲染元信息 / RGBA 输出 / PNG 输出）。
const bisect = await evaluate(`(() => {
  const kernel = window.yanshiKernel;
  const best = (rounds, body) => {
    body();
    let minimum = Infinity;
    for (let index = 0; index < rounds; index++) {
      const started = performance.now();
      body();
      minimum = Math.min(minimum, performance.now() - started);
    }
    return minimum;
  };
  // 逐项 try/catch：任一入口不可用或抛错时，把原因带回 JS 侧而不是整体失败。
  const probe = (rounds, body) => {
    try {
      return best(rounds, body);
    } catch (error) {
      return "err: " + (error && error.message ? error.message : String(error));
    }
  };
  return {
    stats: probe(200, () => kernel.stats()),
    state_json: probe(50, () => kernel.state_json()),
    render_info_8: probe(5, () => kernel.render_region_info(8, 8, 8, 8)),
    render_info_512: probe(5, () => kernel.render_region_info(8, 8, 512, 512)),
    rgba_8: probe(5, () => kernel.render_region_rgba(8, 8, 8, 8)),
    rgba_512: probe(5, () => kernel.render_region_rgba(8, 8, 512, 512)),
    // **拖动反馈那条路**（第 1340 轮定位）：render_region_direct_rgba 走 scratch 直绘
    // （每次重绘区域内所有对象），而上面三项走 tile 组合（复用 LRU）
    // ⇒ 所以**必须先量它**，否则改路之后无法证明收益。
    // 注意：本段在**模板字面量**里 ⇒ 注释里**不能出现反引号**（我在这类坑上又踩了一次）。
    direct_8: probe(5, () => kernel.render_region_direct_rgba(8, 8, 8, 8)),
    direct_512: probe(5, () => kernel.render_region_direct_rgba(8, 8, 512, 512)),
    png_512: probe(5, () => kernel.render_region_png(8, 8, 512, 512)),
  };
})()`);
console.log("固定开销二分（5–200 次取最小）:");
for (const [key, value] of Object.entries(bisect ?? {})) {
  console.log(
    `  ${key.padEnd(16)}: ${typeof value === "number" ? value.toFixed(3) + "ms" : value}`,
  );
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
