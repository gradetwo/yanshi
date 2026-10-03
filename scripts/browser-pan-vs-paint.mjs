#!/usr/bin/env node
// **手形工具：不落笔 ✓ + 仍能平移 ✓**判据（用户报告："平移画布功能异常，小手会像画笔一样画上去" ✓）。
//
// 为什么必须**双向** ✓：只验"不落笔"是不够的 ✗ —— 把平移一起关掉也能让那一半变绿 ✗。
// 为什么必须**先放大** ✗：实测（第 196 轮 ✓）在"画布放得下窗口"时，平移会被钳在原点 ✓
// ⇒ 视图本来就不该变 ⇒ 那个场景下"视图没变"**不是缺陷** ✗（我第一次就差点误判 ✓）。
//
// 用法：node scripts/browser-pan-vs-paint.mjs <viewer-url> <server-base> <token> [cdpPort]
import { strict as assert } from "node:assert";
const [url, base, token, portArg] = process.argv.slice(2);
const port = portArg || "9222";
if (!url || !base || !token) {
  console.error("用法: node scripts/browser-pan-vs-paint.mjs <viewer-url> <server-base> <token> [cdpPort]");
  process.exit(2);
}
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
};
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  socket.send(JSON.stringify({ id, method, params: params || {} }));
});
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
const objects = async () => (await (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool: "list_objects", arguments: {} }),
})).json()).count;
const docId = new URL(url).searchParams.get("doc");
await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await sleep(2600);
// **先放大到 400%** ✗ —— 让画布**超出**窗口，平移才有余量（否则"视图没变"是正常的 ✓）。
await evaluate(`(() => { const input = document.getElementById("zoomInput");
  if (!input) return false; input.value = "400"; input.dispatchEvent(new Event("change", { bubbles: true }));
  input.blur(); return true; })()`);
await sleep(600);
const zoomed = await evaluate(`(window.yanshi.state() || {}).displayScale`);
const picked = await evaluate(`(() => { const button = document.querySelector('button[data-tool="pan"]');
  if (!button) return null; button.click(); return (window.yanshi.state() || {}).tool; })()`);
const before = await objects();
const box = await evaluate(`(() => { const r = document.getElementById("board").getBoundingClientRect();
  return { x: r.left + r.width * 0.3, y: r.top + r.height * 0.5, dx: r.width * 0.25, dy: r.height * 0.2 }; })()`);
const view0 = await evaluate(`(window.yanshi.state() || {}).viewport`);
await send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", buttons: 1, clickCount: 1 });
await sleep(120);
for (let step = 1; step <= 6; step += 1) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved",
    x: box.x + (box.dx * step) / 6, y: box.y + (box.dy * step) / 6, button: "left", buttons: 1 });
  await sleep(60);
}
await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x + box.dx, y: box.y + box.dy, button: "left", buttons: 0, clickCount: 1 });
await sleep(900);
const after = await objects();
const view1 = await evaluate(`(window.yanshi.state() || {}).viewport`);
const moved = view0.x !== view1.x || view0.y !== view1.y;
console.log(`  放大到 ${zoomed}｜选中工具 ${picked}｜对象数 ${before} ⇒ ${after}` +
  `｜视图 (${view0.x.toFixed(0)},${view0.y.toFixed(0)}) ⇒ (${view1.x.toFixed(0)},${view1.y.toFixed(0)})`);
assert.equal(picked, "pan", `应当选中手形工具，实测 ${picked}`);
assert.ok(after <= before, `手形拖动**不该**新增对象：${before} ⇒ ${after}`);
assert.ok(moved, `放大到 ${zoomed} 之后手形拖动**应当**改变视图，但视图没变`);
console.log("  ✓ 手形既不落笔、也仍能平移");
socket.close();
