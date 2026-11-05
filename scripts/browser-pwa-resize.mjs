#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
//
// **★ 总超时守卫 ✗ ★**：浏览器/CDP 起不来时本判据会一直等 ⇒ 超时**报「自己跑不了」并退 3**。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 120000));
__judgeTimeout.unref();

// **PWA 的 `resize_canvas_bounds`：改画布边界必须真的改，而且内容不能被清空** ✓（2026-11-05）
//
// **∴ 守的是什么** ✗：本地层以前**没有这条路由** ✓（501）⇒ 浏览器部署里改不了画布尺寸 ✓。
//   加它比 `flip_canvas` 多一步：**尺寸得先"折"出来** ✓
//   （`p2FoldDocSize` 读最后一条带 `new_width/new_height` 的原子 ✓），
//   否则内核仍按 `create_document` 的旧尺寸建 ⇒ 画布与文档对不上 ✓。
//
// **∴ 四条断言**：
//   ① `get_document` 报的是**新尺寸** ✓（`refreshThumb` 读它 ⇒ 画布板子才会跟着变 ✓）；
//   ② `board.width/height` 真的变成新尺寸 ✓（backing store 必须重建 ✓）；
//   ③ 画布上**仍有墨** ✓（改边界不缩放内容 ⇒ 原内容要留着 ✓；被清空就是数据丢失 ✗）；
//   ④ 原子日志里有 `resize_canvas_bounds` ✓（不是"板子被别的东西改了" ✓）。
//   ＋ 一条负对照：给 `anchor` 必须**明确拒绝** ✓（服务端 `reject_anchor` 同句 ✓，不静默忽略）。
//
// **∴ 变异**：把 `p2FoldDocSize` 从 `get_document` 里去掉 ⇒ ① 红（仍报旧尺寸）✓。
//
// 用法：node scripts/browser-pwa-resize.mjs <viewer-url>
const url = process.env.YANSHI_VIEWER_URL || process.argv[2];
if (!url) { console.error("用法: node scripts/browser-pwa-resize.mjs <viewer-url>"); process.exit(2); }
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

const docId = "pwa-resize-" + Date.now().toString(36);
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

// **先画一笔** ✓：改边界之后"还有墨"才说明内容没被清空 ✓。
const paint = await evaluate(`(async () => {
  const b = document.getElementById("board");
  const r = b.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: r.left + r.width * fx, clientY: r.top + r.height * fy });
  const fire = (type, fx, fy) => b.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 74, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...at(fx, fy),
  }));
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  const color = document.getElementById("color");
  if (color) color.value = "#101010";
  await new Promise((r2) => setTimeout(r2, 250));
  fire("pointerdown", 0.2, 0.2);
  for (let i = 1; i <= 8; i += 1) {
    fire("pointermove", 0.2 + 0.05 * i, 0.2);
    await new Promise((r2) => setTimeout(r2, 25));
  }
  fire("pointerup", 0.6, 0.2);
  return "dispatched";
})()`);
if (paint !== "dispatched") {
  console.log("  ⊘ 不可判：页面里没法落笔（不是产品结论）");
  ws.close();
  process.exit(3);
}

const INK = `(() => {
  const b = document.getElementById("board");
  const d = b.getContext("2d").getImageData(0, 0, b.width, b.height).data;
  let n = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] > 8 && (d[i] < 245 || d[i + 1] < 245 || d[i + 2] < 245)) n += 1;
  }
  return n;
})()`;
const STATE = `(() => {
  const b = document.getElementById("board");
  const st = window.yanshi.state();
  return { docSize: st.docSize, board: { w: b.width, h: b.height },
           viewport: st.viewport, zoom: st.zoom };
})()`;

let before = null;
for (let i = 0; i < 40; i += 1) {
  await sleep(250);
  before = await evaluate(STATE);
  if (before && before.docSize && before.docSize.w === 1024) break;
}
const inkBefore = await evaluate(INK);
console.log("  改之前：" + JSON.stringify(before) + "｜墨 " + inkBefore);
if (!before || !before.docSize || before.docSize.w !== 1024) {
  console.log("  ⊘ 不可判：新文档不是 1024×1024（" + JSON.stringify(before) + "）⇒ 前提没立住");
  ws.close();
  process.exit(3);
}
if (inkBefore <= 50) {
  console.log("  ⊘ 不可判：落笔之后画布上没有墨（" + inkBefore + "）⇒ 前提没立住");
  ws.close();
  process.exit(3);
}

// **负对照**：`anchor` 必须被明确拒绝 ✓（服务端 `reject_anchor` 同句 ✓）。
const neg = await evaluate(`(async () => {
  try {
    const v = await window.yanshiCallTool("resize_canvas_bounds", { width: 640, height: 480, anchor: { x: 0, y: 0 } });
    return { ok: !!(v && v.ok), text: JSON.stringify(v).slice(0, 200) };
  } catch (e) { return { ok: false, thrown: String((e && e.message) || e).slice(0, 120) }; }
})()`);
console.log("  负对照（给 anchor）：" + JSON.stringify(neg).slice(0, 220));

// **改边界** ✓ 640×480（比 1024 小 ⇒ 超出部分渲染时裁掉 ✓）。
const resized = await evaluate(`(async () => {
  try {
    const v = await window.yanshiCallTool("resize_canvas_bounds", { width: 640, height: 480 });
    return { ok: !!(v && v.ok), width: v && v.width, height: v && v.height,
             previous: v && [v.previous_width, v.previous_height], text: JSON.stringify(v).slice(0, 200) };
  } catch (e) { return { ok: false, thrown: String((e && e.message) || e).slice(0, 160) }; }
})()`);
console.log("  resize_canvas_bounds ⇒ " + JSON.stringify(resized).slice(0, 260));

const failures = [];
if (!resized || resized.ok !== true) {
  failures.push(`本地 resize_canvas_bounds 没有成功（${JSON.stringify(resized).slice(0, 200)}）⇒ 这条路由缺失或报错 ✗`);
} else {
  if (resized.width !== 640 || resized.height !== 480) {
    failures.push(`回执里的尺寸不是 640×480（${resized.width}×${resized.height}）✗`);
  }
  if (!Array.isArray(resized.previous) || resized.previous[0] !== 1024) {
    failures.push(`回执里的 previous_width 不是 1024（${JSON.stringify(resized.previous)}）✗`);
  }
}
if (!neg || neg.ok !== false || !neg.text || !neg.text.includes("不接受 anchor")) {
  failures.push(`给 anchor 必须明确拒绝（"不接受 anchor"）✗（实测 ${JSON.stringify(neg).slice(0, 160)}）`);
}

// **等文档尺寸与画布板子都跟上** ✓（有界 10 秒 ✓）。
let after = null;
for (let i = 0; i < 40; i += 1) {
  await sleep(250);
  after = await evaluate(STATE);
  if (after && after.docSize && after.docSize.w === 640 && after.board.w === 640) break;
}
const inkAfter = await evaluate(INK);
console.log("  改之后：" + JSON.stringify(after) + "｜墨 " + inkAfter);
if (!after || !after.docSize || after.docSize.w !== 640 || after.docSize.h !== 480) {
  failures.push(`get_document 没报新尺寸（docSize=${JSON.stringify(after && after.docSize)}，应为 640×480）⇒ 尺寸没被折出来 ✗`);
}
if (!after || after.board.w !== 640 || after.board.h !== 480) {
  failures.push(`画布 backing store 没跟着改（board=${JSON.stringify(after && after.board)}，应为 640×480）✗`);
}
if (inkAfter <= 50) {
  failures.push(`改边界之后画布上的墨没了（${inkBefore} ⇒ ${inkAfter}）⇒ 内容被清空，不是"改边界" ✗`);
}

const atoms = await evaluate(`(async () => {
  try {
    const log = await window.yanshiCallTool("get_log", {});
    return (log.atoms || []).map((a) => a.kind);
  } catch (e) { return ["throw:" + String((e && e.message) || e)]; }
})()`);
if (!Array.isArray(atoms) || !atoms.includes("resize_canvas_bounds")) {
  failures.push(`原子日志里没有 resize_canvas_bounds（${JSON.stringify(atoms)}）⇒ 板子的变化不是这条路由造成的 ✗`);
}

if (failures.length) {
  console.log("  ✗ " + failures.join("；"));
  ws.close();
  process.exit(1);
}
console.log("  ✓ PWA resize_canvas_bounds：尺寸折对了（1024×1024 ⇒ 640×480）、板子跟上、墨还在、锚点被拒");
ws.close();
process.exit(0);
