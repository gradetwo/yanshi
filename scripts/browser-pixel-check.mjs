#!/usr/bin/env node
// 在真实 Chromium 里做「内核 vs 服务端」逐像素自检，按设计 6.1 的 D1 口径判定：
// 差异像素数极少（≤16）且最大通道差 ≤1 LSB 记为通过（判定逻辑在编辑器内的 checkBitExact）。
//
// 前置条件：
//   1) 已运行 yanshi 服务端（默认 127.0.0.1:8110）；
//   2) 已启动带远程调试的 Chromium：
//        chromium --remote-debugging-port=9333 --headless=new about:blank
//
// 用法：
//   node scripts/browser-pixel-check.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..."
//
// 说明：探测的是编辑器自身的自检结果（window.yanshiStats），因此它验证的是**真实用户路径**
// （WASM 内核 + 服务端渲染 + 浏览器合成），不是另写一套比对逻辑。

const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: node scripts/browser-pixel-check.mjs <viewer-url>");
  process.exit(2);
}

const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到可用的调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
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

// 等 WASM 内核就绪且拿到服务端 head。
let stats = null;
for (let attempt = 0; attempt < 150; attempt++) {
  stats = await evaluate("window.yanshiStats");
  if (stats?.wasm && stats.kernelHead > 0) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
if (!stats?.wasm) {
  console.error("WASM 内核未就绪（检查服务端 /health 的 wasm 字段与浏览器控制台）");
  process.exit(1);
}
// 触发编辑器内置的位精确自检。
await evaluate('document.querySelector(\'button[data-tool="check"]\').click()');
let out = null;
for (let attempt = 0; attempt < 150; attempt++) {
  out = await evaluate("window.yanshiStats");
  if (out?.diffPixels !== undefined) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
if (!out || out.diffPixels === undefined) {
  console.error("自检未返回结果（编辑器可能仍在渲染，或内核无输出）");
  process.exit(1);
}
const doc = new URL(url).searchParams.get("doc") ?? "(unknown)";
const verdict = out.bitExact ? "通过" : "不通过";
console.log(
  `文档 ${doc}: HEAD ${out.kernelHead}/${out.serverHead} | 差异像素 ${out.diffPixels} | ` +
    `最大通道差 ${out.maxChannelDelta} | 判定 ${verdict}`,
);
ws.close();
process.exit(out.bitExact ? 0 : 1);
