#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
//
// **★ 总超时守卫 ✗ ★**：浏览器/CDP 起不来时本判据会一直等 ⇒ 超时**报「自己跑不了」并退 3**。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 120000));
__judgeTimeout.unref();

// **PWA 的 `flip_canvas`（整幅左右翻）必须真的翻** ✓（2026-11-05）
//
// **∴ 守的是什么** ✗：本地层以前**没有这条路由** ✓（`not_implemented_locally` ⇒ 501）
//   ⇒ 创作者/Agent 在只有浏览器的部署里**翻不了画布** ✓（服务端有、本地没有 = 三方缺口 ✓）。
//
// **∴ 三条断言（每条都能单独变红 ✓）**：
//   ① **翻之前**：墨在**左半边**（`left > right`）✓ —— 前提，不成立就不可判 ✓；
//   ② **翻之后**：墨搬到**右半边** ✓（`right > left`）且总量基本不变（≥ 九成 ✓，
//      镜像不会增减墨 ⇒ 少一大截说明整块被裁掉了 ✓）；
//   ③ **日志里真有 `flip_canvas` 原子** ✓ —— 不是"画布被别的东西重画了一遍"✓。
//
// **∴ 为什么不用"两次截图逐字节相等"** ✗：镜像后的画布与"逐字节取反"不是同一件事 ✓
//   （抗锯齿、坐标取整都会差 ✓）⇒ 判**搬没搬过去**才是这条路由的语义 ✓。
//
// 用法：node scripts/browser-pwa-flip-canvas.mjs <viewer-url>
const url = process.env.YANSHI_VIEWER_URL || process.argv[2];
if (!url) { console.error("用法: node scripts/browser-pwa-flip-canvas.mjs <viewer-url>"); process.exit(2); }
const port = process.env.CDP_PORT || "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）"); process.exit(3); }

const ws = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
ws.onmessage = (event) => {
  const m = JSON.parse(event.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
await new Promise((open) => { ws.onopen = open; });
const send = (method, params) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  ws.send(JSON.stringify({ id, method, params: params || {} }));
});
const evaluate = async (expression) => {
  const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const details = reply?.result?.exceptionDetails;
  if (details) {
    const text = details.exception?.description || details.exception?.value || details.text || "（无描述）";
    throw new Error("页面侧抛错：" + String(text).split("\n").slice(0, 3).join(" ｜ "));
  }
  return reply?.result?.result?.value;
};
await send("Runtime.enable");
await send("Page.enable");

const docId = "pwa-flip-" + Date.now().toString(36);
const sep = url.includes("?") ? "&" : "?";
await send("Page.navigate", { url: `${url}${sep}doc=${docId}&token=local&render=client` });
for (let i = 0; i < 80; i += 1) {
  await sleep(250);
  try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("board")')) break; } catch (_) { /* 还没就绪 */ }
}
let kernelReady = false;
for (let i = 0; i < 80; i += 1) {
  if (await evaluate("!!(window.yanshi && window.yanshi.kernelStats && window.yanshi.kernelStats())")) { kernelReady = true; break; }
  await sleep(250);
}
if (!kernelReady) {
  console.log("  ⊘ 不可判：内核 20 秒内没就绪 ⇒ 本条本次没验（不是产品结论）");
  ws.close();
  process.exit(3);
}

// **墨的左右分布** ✓：按"非背景像素"数，左右各一半视口 ✓。
const HALVES = `(() => {
  const b = document.getElementById("board");
  const ctx = b.getContext("2d");
  const d = ctx.getImageData(0, 0, b.width, b.height).data;
  let left = 0, right = 0;
  const mid = b.width / 2;
  for (let i = 0; i < d.length; i += 4) {
    if (!(d[i + 3] > 8)) continue;
    if (d[i] > 245 && d[i + 1] > 245 && d[i + 2] > 245) continue;
    const x = (i / 4) % b.width;
    if (x < mid) left += 1; else right += 1;
  }
  return { left, right, total: left + right, w: b.width, h: b.height };
})()`;

const paint = await evaluate(`(async () => {
  const b = document.getElementById("board");
  const r = b.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: r.left + r.width * fx, clientY: r.top + r.height * fy });
  const fire = (type, fx, fy) => b.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 73, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...at(fx, fy),
  }));
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  const color = document.getElementById("color");
  if (color) color.value = "#101010";
  const size = document.getElementById("size");
  if (size) size.value = "40";
  await new Promise((r2) => setTimeout(r2, 250));
  // **竖着画在左半边** ✓（镜像后左右可分辨 ✓；上下不受影响 ✓）。
  fire("pointerdown", 0.22, 0.25);
  for (let i = 1; i <= 10; i += 1) {
    fire("pointermove", 0.22, 0.25 + 0.05 * i);
    await new Promise((r2) => setTimeout(r2, 25));
  }
  fire("pointerup", 0.22, 0.75);
  return "dispatched";
})()`);
if (paint !== "dispatched") {
  console.log("  ⊘ 不可判：页面里没法落笔（不是产品结论）");
  ws.close();
  process.exit(3);
}

const failures = [];
let before = null;
for (let i = 0; i < 40; i += 1) {
  await sleep(250);
  before = await evaluate(HALVES);
  if (before && before.total > 200) break;
}
console.log("  翻之前：" + JSON.stringify(before));
if (!before || before.total <= 200) {
  console.log("  ⊘ 不可判：画布上没有足够的墨（" + JSON.stringify(before) + "）⇒ 前提没立住");
  ws.close();
  process.exit(3);
}
if (!(before.left > before.right)) {
  failures.push(`翻之前墨不在左半边（left=${before.left} right=${before.right}）⇒ 前提不成立，本条不可判`);
}

// **翻** ✓：走产品自己的工具入口（与 Agent / UI 同一条路 ✓）。
const flipped = await evaluate(`(async () => {
  try {
    const v = await window.yanshiCallTool("flip_canvas", { direction: "horizontal" });
    return { ok: !!(v && v.ok), raw: JSON.stringify(v).slice(0, 300) };
  } catch (e) { return { ok: false, thrown: String((e && e.message) || e).slice(0, 200) }; }
})()`);
console.log("  flip_canvas ⇒ " + JSON.stringify(flipped).slice(0, 300));
if (!flipped || flipped.ok !== true) {
  failures.push(`本地 flip_canvas 没有成功（${JSON.stringify(flipped)}）⇒ 这条路由缺失或报错 ✗`);
}

let after = null;
for (let i = 0; i < 40; i += 1) {
  await sleep(250);
  after = await evaluate(HALVES);
  if (after && after.right > after.left && after.total > 200) break;
}
console.log("  翻之后：" + JSON.stringify(after));
if (!after || after.total <= 200) {
  failures.push(`翻之后画布上的墨没了（${JSON.stringify(after)}）⇒ 翻转把内容清空了 ✗`);
} else {
  if (!(after.right > after.left)) {
    failures.push(`翻之后墨没有搬到右半边（left=${after.left} right=${after.right}）⇒ 画布没有重画翻转后的状态 ✗`);
  }
  if (after.total < before.total * 0.9) {
    failures.push(`翻之后墨少了（${before.total} ⇒ ${after.total}，低于九成）⇒ 内容被裁掉，不是镜像 ✗`);
  }
}

// ③ **日志里真有那个原子** ✓
const atoms = await evaluate(`(async () => {
  try {
    const log = await window.yanshiCallTool("get_log", {});
    return (log.atoms || []).map((a) => a.kind);
  } catch (e) { return ["throw:" + String((e && e.message) || e)]; }
})()`);
console.log("  日志原子：" + JSON.stringify(atoms));
if (!Array.isArray(atoms) || !atoms.includes("flip_canvas")) {
  failures.push(`原子日志里没有 flip_canvas（${JSON.stringify(atoms)}）⇒ 画布的变化不是这条路由造成的 ✗`);
}

if (failures.length) {
  console.log("  ✗ " + failures.join("；"));
  ws.close();
  process.exit(1);
}
console.log("  ✓ PWA flip_canvas：墨从左半边搬到右半边，日志里有 flip_canvas 原子");
ws.close();
process.exit(0);
