#!/usr/bin/env node
// 真实 Chromium 里的**查看器 UI 回归检查**（针对实际使用中报告的问题）：
//
//   1. 画一笔之后，内容画布**不应变空白**（提交后仍能看到结果）；
//   2. 内容画布与显示区域**几何一致**（不再出现「右边一块颜色不同、点了没反应」）；
//   3. 提交后**缩略图自动变化**（不需要手动点刷新）；
//   4. 记录过程中出现的控制台错误与 'tiles 个失效' 噪声行数。
//
// 前置：服务端在跑；`chromium --remote-debugging-port=9333 --headless=new about:blank`。
// 用法：node scripts/browser-ui-check.mjs "http://127.0.0.1:8110/?doc=ui&token=..."
const url = process.argv[2];
// 总体超时：脚本自身不设限会在调试目标无响应时永久挂住（CI 上尤其糟）。
const deadline = Date.now() + Number(process.env.UI_TIMEOUT_MS || 180000);
const checkDeadline = () => {
  if (Date.now() > deadline) {
    console.error("❌ UI 检查超时（调试目标无响应？）");
    process.exit(3);
  }
};
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: node scripts/browser-ui-check.mjs <viewer-url>");
  process.exit(2);
}

const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
const consoleLines = [];
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.consoleAPICalled") {
    const text = (message.params.args || []).map((arg) => arg.value ?? "").join(" ");
    consoleLines.push(text);
  }
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
for (let attempt = 0; attempt < 150; attempt++) {
  checkDeadline();
  const stats = await evaluate("window.yanshiStats");
  if (stats?.wasm && stats.kernelHead > 0) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}

// 画布非空判定：统计**不透明**像素（空白画布是全透明；只有白底也算空）。
const blankCheck = `(() => {
  const board = document.getElementById("board");
  const ctx = board.getContext("2d");
  const data = ctx.getImageData(0, 0, board.width, board.height).data;
  let opaque = 0;
  let painted = 0;
  for (let i = 0; i < data.length; i += 4) {
    const alpha = data[i+3];
    if (alpha > 8) opaque++;
    if (alpha > 8 && (data[i] < 245 || data[i+1] < 245 || data[i+2] < 245)) painted++;
  }
  return { opaque, painted, width: board.width, height: board.height };
})()`;

// 用真实指针事件在画布上画一笔（与用户操作同一条路径）。
const drawStroke = `(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const send = (type, point) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 1, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...point,
  }));
  const points = [[0.25, 0.3], [0.45, 0.45], [0.65, 0.35]].map(([x, y]) => at(x, y));
  send("pointerdown", points[0]);
  for (const point of points.slice(1)) { send("pointermove", point); await new Promise(r => setTimeout(r, 60)); }
  send("pointerup", points[points.length - 1]);
  await new Promise(r => setTimeout(r, 1200));
  return true;
})()`;

if (process.env.UI_TRACE === "1") await evaluate("window.yanshiStats.tracePaints = true");
const before = await evaluate(blankCheck);
const layerCountBefore = await evaluate(`document.getElementById("layer").options.length`);
await evaluate(`document.getElementById("addLayer").click()`);
await new Promise((resolve) => setTimeout(resolve, 1200));
const layerCountAfter = await evaluate(`document.getElementById("layer").options.length`);
await evaluate(drawStroke);
const after = await evaluate(blankCheck);
// 画完之后随时间采样：若着色像素先出现再消失，说明有「后到的整幅绘制」把内容覆盖了。
const timeline = [];
for (let step = 0; step < 6; step++) {
  const sample = await evaluate(blankCheck);
  timeline.push(sample.painted);
  await new Promise((resolve) => setTimeout(resolve, 250));
}

const geometry = await evaluate(`(() => {
  const board = document.getElementById("board");
  const overlay = document.getElementById("overlay");
  const preview = document.getElementById("preview");
  const rect = board.getBoundingClientRect();
  const overlayRect = overlay ? overlay.getBoundingClientRect() : null;
  return {
    canvas: { w: board.width, h: board.height, cssW: Math.round(rect.width), cssH: Math.round(rect.height) },
    overlay: overlayRect ? { cssW: Math.round(overlayRect.width), cssH: Math.round(overlayRect.height) } : null,
    preview: preview ? {} : null,
  };
})()`);

const thumbBefore = await evaluate(`document.getElementById("thumb").src`);
await evaluate(`(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const point = { clientX: rect.left + rect.width * 0.5, clientY: rect.top + rect.height * 0.7 };
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true, pointerId: 2, pointerType: "mouse", isPrimary: true, buttons: 1, ...point }));
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true, pointerId: 2, pointerType: "mouse", isPrimary: true, buttons: 0, ...point }));
  await new Promise(r => setTimeout(r, 1500));
  return true;
})()`);
const thumbAfter = await evaluate(`document.getElementById("thumb").src`);
const tileNoise = consoleLines.filter((line) => String(line).includes("个失效")).length;

const problems = [];
if (after.painted === 0) {
  problems.push(`操作后画布上没有已绘制的像素（不透明 ${after.opaque}，着色 ${after.painted}，操作前不透明 ${before.opaque}）`);
}
if (layerCountAfter <= layerCountBefore) {
  problems.push(`「＋ 图层」按钮没有新增图层（之前 ${layerCountBefore} 个，之后 ${layerCountAfter} 个）`);
}
// 内容画布与覆盖层必须几何一致（覆盖层若为空则说明没有覆盖 img，属正常）。
if (geometry.overlay && (geometry.overlay.cssW !== geometry.canvas.cssW || geometry.overlay.cssH !== geometry.canvas.cssH)) {
  problems.push(`内容画布与覆盖层几何不一致：canvas ${geometry.canvas.cssW}×${geometry.canvas.cssH} vs overlay ${geometry.overlay.cssW}×${geometry.overlay.cssH}`);
}
if (geometry.preview !== null) {
  problems.push("页面里仍存在覆盖用的 #preview 元素（会与画布几何冲突）");
}
if (thumbBefore === thumbAfter) problems.push("提交后缩略图未自动刷新");

if (process.env.UI_DEBUG === "1") {
  console.log("  --- 调试 ---");
  console.log("  stats:", JSON.stringify(await evaluate("window.yanshiStats")));
  console.log("  日志:", JSON.stringify(await evaluate(`document.getElementById("log").innerText.slice(-800)`)));
  console.log("  最近响应:", JSON.stringify(await evaluate(`document.getElementById("last").innerText.slice(0, 400)`)));
  console.log("  状态栏:", JSON.stringify(await evaluate(`document.getElementById("status") ? document.getElementById("status").innerText : ""`)));
}
console.log(`  画布：操作前不透明 ${before.opaque}/着色 ${before.painted}｜操作后不透明 ${after.opaque}/着色 ${after.painted}`);
console.log(`  图层数：${layerCountBefore} → ${layerCountAfter}`);
console.log(`  着色像素时间线（每 250ms）：${timeline.join(" → ")}`);
console.log(`  几何：canvas ${geometry.canvas.w}×${geometry.canvas.h}（CSS ${geometry.canvas.cssW}×${geometry.canvas.cssH}）｜preview ${JSON.stringify(geometry.preview)}`);
console.log(`  缩略图：${thumbBefore === thumbAfter ? "未变化" : "已自动刷新"}`);
console.log(`  'tiles 个失效' 噪声行：${tileNoise}`);
if (problems.length) {
  console.log(`  ❌ ${problems.length} 项不合格：`);
  for (const problem of problems) console.log(`     - ${problem}`);
  process.exit(1);
}
console.log("  ✅ UI 检查通过（画布非空、几何一致、缩略图自动刷新）");
