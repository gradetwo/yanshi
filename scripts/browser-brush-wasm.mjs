#!/usr/bin/env node
// **门面 wasm 与 `.myb` 文本能从浏览器拿到，并且真的能画** ✓ —— 接线第 1、2 步的判据 ✓。
//
// **判据（都能红 ✓）**：
//   ① `GET /brush-module.wasm` ⇒ 能 `WebAssembly.instantiate` ✓；
//   ② `GET /brushes/<name>.myb` ⇒ 拿到真实文本 ✓，喂给门面之后**落下墨**且**颜色对** ✓
//      （不是"fetch 返回 200"就算 ✓ —— 那是本项目最反对的判据 ✗）；
//   ③ **越权取文件必须被拒** ✓：带 `..` 或非 `.myb` 的名字不许成功 ✓。
//
// 用法：CDP_PORT=8523 node scripts/browser-brush-wasm.mjs "<viewer-url>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-brush-wasm.mjs <viewer-url>"); process.exit(2); }
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page") || null;
if (!target) { console.error("未找到调试目标"); process.exit(2); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1; const pending = new Map();
ws.addEventListener("message", (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) => new Promise((resolve) => { const i = id++; pending.set(i, resolve); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await send("Runtime.enable"); await send("Page.enable");
await send("Page.navigate", { url }); await send("Page.reload", { ignoreCache: true }); await sleep(1800);

const result = await evaluate(`(async () => {
  const moduleBytes = await fetch("/brush-module.wasm").then((r) => r.ok ? r.arrayBuffer() : null);
  if (!moduleBytes) return { error: "/brush-module.wasm 取不到" };
  const { instance } = await WebAssembly.instantiate(moduleBytes, {});
  const api = instance.exports;
  const myb = await fetch("/brushes/" + "spray.myb").then((r) => r.ok ? r.text() : null);
  if (!myb) return { error: "/brushes/spray.myb 取不到" };
  const request = JSON.stringify({
    myb, points: [[40, 40, 1], [80, 40, 1]], size: 40,
    color: { r: 255, g: 0, b: 0, a: 255 }, region: { x: 0, y: 0, w: 128, h: 96 },
  });
  const payload = new TextEncoder().encode(request);
  const pointer = api.yanshi_brush_alloc(payload.length);
  new Uint8Array(api.memory.buffer, pointer, payload.length).set(payload);
  const outLen = api.yanshi_brush_paint(pointer, payload.length);
  api.yanshi_brush_free(pointer, payload.length);
  const view = outLen ? new Uint8Array(api.memory.buffer, api.yanshi_brush_out_ptr(), outLen).slice() : null;
  let ink = 0, reddest = 0;
  if (view) for (let i = 0; i + 3 < view.length; i += 4) { if (view[i + 3] > 32) ink += 1; if (view[i + 3] > 200 && view[i] > reddest) reddest = view[i]; }
  const badDotDot = await fetch("/brushes/" + "..%2fCargo.toml").then((r) => r.status);
  const badExtension = await fetch("/brushes/100%25_Opaque.txt").then((r) => r.status);
  return {
    moduleBytes: moduleBytes.byteLength, outLen, expected: 128 * 96 * 4, ink, reddest,
    badDotDot, badExtension, mybLength: myb.length,
  };
})()`);
console.log("  " + JSON.stringify(result));
if (!result || result.error) { console.error("❌ " + (result?.error || "探针失败")); process.exit(1); }
if (result.outLen !== result.expected || result.ink < 100 || result.reddest < 240) {
  console.error("❌ 门面在页面里没能画出正确的墨/颜色 ✗"); process.exit(1);
}
if (result.badDotDot === 200 || result.badExtension === 200) {
  console.error(`❌ 越权取文件居然成功了（.. ⇒ ${result.badDotDot}、非 .myb ⇒ ${result.badExtension}）✗`); process.exit(1);
}
console.log(`  ✅ 模块 ${result.moduleBytes} 字节、能实例化、落墨 ${result.ink}、最红 R=${result.reddest}；越权被拒 ✓`);
ws.close();
