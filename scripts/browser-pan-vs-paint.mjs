#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）

// **★ 总超时守卫 ✗ ★**（2026-10-16 ✓）：**本判据要连 CDP 浏览器** ✓ ⇒ 浏览器起不来时它可能**一直等**
//   （实测：CI 的判据分片被这种判据**拖到跑不完** ✗ ⇒ 我这边十几轮拿不到判定 ✓）。
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✗，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
//   ⇒ **∴ 超时就报「无法判断」并退 3** ✓ —— **不冒充产品失败** ✗、**也不永远挂着** ✗ ★
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 90000);
// **★ 必须 `unref` ✗ ★**（外§117⑨ 的**第二个实例** ✓；**CI 实测换来的**✗）：
//   **∴ 不解除** ⇒ **跑完之后** node 仍被这个定时器拖着 ✓ ⇒ 到点就 `process.exit(3)` ✗
//   ⇒ **∴ 于是** CI 板面上「**✓ 断言全过**」后面跟着 **EXIT=3**（**三条判据就这样长期没在验东西** ✓）
//   **∴ `unref` 之后**：成功路径**自然退出** ✓；而**真的卡住**时事件循环仍活着 ⇒ 守卫**照样到点**报 3 ✓
__judgeTimeout.unref();
// **手形工具：不落笔 ✓ + 仍能平移 ✓**判据（用户报告："平移画布功能异常，小手会像画笔一样画上去" ✓）。
//
// 为什么必须**双向** ✓：只验"不落笔"是不够的 ✗ —— 把平移一起关掉也能让那一半变绿 ✗。
// 为什么必须**先放大** ✗：实测（第 196 轮 ✓）在"画布放得下窗口"时，平移会被钳在原点 ✓
// ⇒ 视图本来就不该变 ⇒ 那个场景下"视图没变"**不是缺陷** ✗（我第一次就差点误判 ✓）。
//
// 用法：node scripts/browser-pan-vs-paint.mjs <viewer-url> <server-base> <token> [cdpPort]
import { strict as assert } from "node:assert";
const [url, base, token, portArg] = process.argv.slice(2);
// **端口只认纯数字** ✓ ＋ **`CDP_PORT` 优先** ✓（与这一族其余判据同规 ⇒ 守卫见
//   `check-browser-cdp-port-source.mjs`：`argv[3]` 是**服务端 base**（URL）不是端口 ✗）
const port = process.env.CDP_PORT || (/^\d+$/.test(portArg || "") ? portArg : "9222");
if (!url || !base || !token) {
  console.error("用法: node scripts/browser-pan-vs-paint.mjs <viewer-url> <server-base> <token> [cdpPort]");
  process.exit(2);
}
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
// **★ CDP 连接守卫 ✗ ★**（2026-10-16 ✓）：浏览器没起来时这里会抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
let targets;
try {
  targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
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
// **等到"页面就绪"这个独立信号** ✓（第 895 轮 ✓）：原来固定睡 2.6 秒 ✗ ⇒ 机器慢时下面 `#zoomInput`
// 会取不到 ⇒ `:46` 直接 `return false` ⇒ 判据间歇红 ✗（**"取不到就静默 false"正是间歇的机制** ✓）。
// ⚠️ **等待条件与断言条件不同** ✗：这里等 `readyState` ＋ **`#zoomInput` 存在** ✓，
//    而断言读的是 **`displayScale` 的值** ✓ ✓（**等"值"就等于让断言永不失败** ✗ —— 第 887 轮的教训 ✓）。
// ⚠️ **"元素存在"还不够强** ✗（第 925 轮 ✓，本地实测抓到 ✓）：
// 只等 `#zoomInput` 存在 ⇒ **它的事件处理可能还没接上** ✗ ⇒ 下面 `dispatchEvent("change")` 被忽略 ✗
// ⇒ **缩放停在适配值（实测 15.9 ✗，而意图是 400 ✓）** ⇒ 画布全可见 ⇒ **平移被夹 ⇒ `:79` 断言失败** ✗。
// ⇒ 所以：**轮询存在性 ＋ 有界沉降** ✓（**让接线跑完 ✓**），**而"缩放是否真的变了"仍由断言判** ✗ ✓。
let readyForZoom = false;
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("zoomInput")')) { readyForZoom = true; break; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
if (!readyForZoom) console.log("  ⚠ 等待 #zoomInput 超时 ⇒ 下面的缩放可能不生效（**但结论仍由断言给出** ✓）");
await sleep(800);
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
