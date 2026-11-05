#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
//
// **★ 总超时守卫 ✗ ★**：浏览器/CDP 起不来时本判据会一直等 ⇒ 超时**报「自己跑不了」并退 3**
//   （与产品失败区分；`unref()` ⇒ 跑完不被它拖住）。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 120000));
__judgeTimeout.unref();

// **PWA 的撤销/重做必须真的能用** ✓（2026-11-05 ✓）
//
// **∴ 守的是什么** ✗：静态 PWA 里画完一笔之后，
//   ① 状态栏必须写「可撤销 ≥ 1 笔」（不是 0），且撤销按钮**可用**；
//   ② 点撤销 ⇒ 画布上的墨必须**真的消失**；
//   ③ 撤完之后重做那一格必须变成 ≥ 1、重做按钮可用。
//
// **∴ 为什么需要它** ✗：这条链在 2026-11-05 之前**整条是断的** ✓ ——
//   本地 `draw_stroke` 的响应是 `{ok:true, atoms:[{id:…}]}` ✓
//   而前端只读 `value.atom_id` ✗ ⇒ `afterMutation(null)` ⇒ 撤销栈不涨、也不刷新只读状态 ✓
//   ⇒ **画完一笔界面仍写「可撤销 0 笔」、撤销按钮永远是灰的** ✓
//   ⇒ 创作者**画完不能撤销** ✓（而 `get_undo_status` 一直如实回 2 ⇒ 界面与事实不一致 ✗）。
//   **∴ 这条判据要能在**恢复旧写法时变红**✗**：把 `responseAtomIdOf(value)` 换回
//   `value.atom_id` ⇒ ① 直接失败（退出码 1）✓。
//
// **∴ 前提**：查的是**静态 PWA**（`__pwaLocalOnly`）—— 只有它走本地 `draw_stroke`。
//   `run-criteria.sh` 会给 `browser-*` 一个 viewer URL（有静态 PWA 时就是它 ✓）。
//
// 用法：node scripts/browser-pwa-undo.mjs <viewer-url> [base] [token] [cdpPort]
const url = process.env.YANSHI_VIEWER_URL || process.argv[2];
if (!url) { console.error("用法: node scripts/browser-pwa-undo.mjs <viewer-url>"); process.exit(2); }
const port = process.env.CDP_PORT || process.argv[5] || "9333";
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
// **`Runtime.evaluate` 必须同时给两个开关**（AGENTS.md）：只给 `returnByValue` 时
// async 表达式会**静默返回 `{}`** ⇒ 判据会误判成产品 bug。
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

const docId = "pwa-undo-" + Date.now().toString(36);
const sep = url.includes("?") ? "&" : "?";
await send("Page.navigate", { url: `${url}${sep}doc=${docId}&token=local&render=client` });
for (let i = 0; i < 80; i += 1) {
  await sleep(250);
  try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("board")')) break; } catch (_) { /* 还没就绪 */ }
}
// **内核必须真的起来**（笔走本地内核；没起来时手势会被吞掉 ⇒ 那是前提，不是结论）。
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

const READ = `(() => {
  const u = document.querySelector('button[data-tool="undo"]');
  const r = document.querySelector('button[data-tool="redo"]');
  const label = document.getElementById("undoDepth");
  const text = label ? label.textContent : "";
  const m = /可撤销\\s*(\\S+)\\s*笔\\s*\\/\\s*可重做\\s*(\\S+)\\s*笔/.exec(text);
  const num = (v) => (/^[0-9]+$/.test(String(v)) ? Number(v) : null);
  return { text, undo: m ? num(m[1]) : null, redo: m ? num(m[2]) : null,
           undoDisabled: u ? u.disabled : "no-button", redoDisabled: r ? r.disabled : "no-button" };
})()`;
const INK = `(() => {
  const b = document.getElementById("board");
  const d = b.getContext("2d").getImageData(0, 0, b.width, b.height).data;
  let n = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] > 8 && (d[i] < 245 || d[i + 1] < 245 || d[i + 2] < 245)) n += 1;
  }
  return n;
})()`;

const failures = [];
const boot = await evaluate(READ);
console.log("  开机： " + JSON.stringify(boot));
if (boot.undo !== 0) failures.push(`空文档的状态栏没写「可撤销 0 笔」（是 ${JSON.stringify(boot.text)}）⇒ 基线不可信 ✗`);
if (boot.undoDisabled !== true) failures.push(`空文档的撤销按钮应当禁用（实际 ${JSON.stringify(boot.undoDisabled)}）✗`);

// **画一笔**：页面内真实 pointer 手势（与创作者的操作同一条路径）。
const PAINT = `(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, fx, fy) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 77, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...at(fx, fy),
  }));
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  const color = document.getElementById("color");
  if (color) color.value = "#101010";
  await new Promise((r) => setTimeout(r, 250));
  fire("pointerdown", 0.25, 0.45);
  for (let i = 1; i <= 8; i += 1) {
    fire("pointermove", 0.25 + 0.05 * i, 0.45);
    await new Promise((r) => setTimeout(r, 30));
  }
  fire("pointerup", 0.65, 0.45);
  return "dispatched";
})()`;
if (await evaluate(PAINT) !== "dispatched") {
  console.log("  ⊘ 不可判：页面里没法落笔（不是产品结论）");
  ws.close();
  process.exit(3);
}

// ① 画完之后：墨 > 0、状态栏 ≥ 1、按钮可用（**有界轮询**：落笔到状态刷新是异步的）。
let afterPaint = null;
let inkAfterPaint = 0;
for (let i = 0; i < 40; i += 1) {
  afterPaint = await evaluate(READ);
  inkAfterPaint = await evaluate(INK);
  if (typeof afterPaint.undo === "number" && afterPaint.undo >= 1) break;
  await sleep(250);
}
console.log("  画完： " + JSON.stringify(afterPaint) + "｜墨 " + inkAfterPaint);
if (inkAfterPaint <= 50) failures.push(`画完之后画布上没有墨（${inkAfterPaint} 个像素）⇒ 落笔没生效，后面的断言没有意义 ✗`);
if (!(typeof afterPaint.undo === "number" && afterPaint.undo >= 1)) {
  failures.push(`画完一笔后状态栏没有报可撤销（${JSON.stringify(afterPaint.text)}）⇒ 撤销栈与事实不一致 ✗`);
}
if (afterPaint.undoDisabled !== false) {
  failures.push(`画完一笔后撤销按钮仍不可用（disabled=${JSON.stringify(afterPaint.undoDisabled)}）⇒ 创作者画完不能撤销 ✗`);
}

// ② 点撤销 ⇒ 墨必须**真的**消失（不是只改一行字）。
await evaluate(`(() => { const b = document.querySelector('button[data-tool="undo"]'); if (b) b.click(); return !!b; })()`);
let afterUndo = null;
let inkAfterUndo = inkAfterPaint;
for (let i = 0; i < 40; i += 1) {
  await sleep(250);
  afterUndo = await evaluate(READ);
  inkAfterUndo = await evaluate(INK);
  if (inkAfterUndo <= 50 && afterUndo.redo >= 1) break;
}
console.log("  撤销后：" + JSON.stringify(afterUndo) + "｜墨 " + inkAfterUndo);
if (inkAfterUndo > 50) failures.push(`点了撤销，画布上还剩 ${inkAfterUndo} 个墨像素（画之前 ${inkAfterPaint}）⇒ 撤销没有真的撤 ✗`);
if (!(typeof afterUndo.redo === "number" && afterUndo.redo >= 1)) {
  failures.push(`撤完之后状态栏没有报可重做（${JSON.stringify(afterUndo.text)}）⇒ 重做那一格没跟着更新 ✗`);
}
if (afterUndo.redoDisabled !== false) {
  failures.push(`撤完之后重做按钮仍不可用（disabled=${JSON.stringify(afterUndo.redoDisabled)}）✗`);
}

if (failures.length) {
  console.log("  ✗ " + failures.join("；"));
  ws.close();
  process.exit(1);
}
console.log("  ✓ PWA 撤销/重做：状态栏、按钮与画布三者一致（画 1 笔 ⇒ 可撤销 1 ⇒ 撤销后墨归零 ⇒ 可重做 1）");
ws.close();
process.exit(0);
