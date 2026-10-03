#!/usr/bin/env node
// **离线画一笔**判据（目标 (A)③⑥）：断网后仍必须能在画布上画出墨。
// 为什么它是"离线优先"的成功定义：能"打开页面"只是外壳（已达成 ✓），
// 真正的要求是**离线下还能画**（本地渲染 + 本地状态，服务端不在也算数）。
// 用法：node scripts/browser-offline-draw.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
const port = process.argv[3] || process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-offline-draw.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
await send("Page.navigate", { url });
await sleep(3000);
// **墨量**：把画布上偏暗的像素数出来（与判据无关的具体画法无关 ✓）
const INK = `(() => {
  // **按 alpha 数"真的上了墨"的像素** ✗ —— 第一版按 RGB 数 ✗ ⇒ 透明画布的 (0,0,0,0)
  // 也被算成"墨" ✓ ⇒ 得到"墨 = 900×640 = 整块画布"✗ ⇒ 那个"增长"是噪声、绿是**假的** ✗。
  const canvases = Array.from(document.querySelectorAll("canvas"));
  const parts = [];
  let ink = 0;
  for (const canvas of canvases) {
    if (!canvas.width || !canvas.height) { parts.push({ id: canvas.id || "?", ink: 0, note: "无尺寸" }); continue; }
    let data = null;
    try { data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data; } catch (error) { parts.push({ id: canvas.id || "?", ink: 0, note: "读不到像素" }); continue; }
    let mine = 0;
    // **只看"非白且不透明"的像素** ✗ —— `board` 是整块不透明白底 ✓ ⇒ 单看 alpha 会恒等于画布面积 ✗
    // （第一版就是这样：离线/在线都 576000 ✓ ⇒ 信号被淹没 ✗）。真正说明"上了墨"的是
    // **overlay 上那些非白像素** ✓（实测：在线 2046 ✓、离线 0 ✗）。
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 8 && (data[i] < 245 || data[i + 1] < 245 || data[i + 2] < 245)) mine += 1;
    }
    parts.push({ id: canvas.id || "?", ink: mine });
    ink += mine;
  }
  return { canvases: canvases.length, parts, ink };
})()`;
// **画一笔**：合成指针事件（本地渲染路径看得见它们 ✓；服务端提交在离线时必然失败 ✓，正是要测的点 ✓）
const STROKE = `(() => {
  const board = document.getElementById("board");
  if (!board) return { ok: false, why: "没有 #board" };
  const rect = board.getBoundingClientRect();
  const at = (fx, fy, extra) => Object.assign({
    clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy,
    bubbles: true, pointerId: 1, isPrimary: true, button: 0, buttons: 1, pressure: 0.7, pointerType: "pen",
  }, extra || {});
  board.dispatchEvent(new PointerEvent("pointerdown", at(0.3, 0.4)));
  for (let step = 1; step <= 6; step += 1) {
    board.dispatchEvent(new PointerEvent("pointermove", at(0.3 + 0.05 * step, 0.4 + 0.03 * step)));
  }
  board.dispatchEvent(new PointerEvent("pointerup", at(0.6, 0.58, { buttons: 0 })));
  return { ok: true, brush: (document.getElementById("brush") || {}).value || "" };
})()`;
// ① 在线：先画一笔（让资产/门面有机会被加载与缓存 ✓）
const online = await evaluate(STROKE);
await sleep(1500);
const inkOnline = await evaluate(INK);
// ② 断网 ⇒ 再画一笔 ⇒ **必须有墨**
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
const offlineStroke = await evaluate(STROKE);
await sleep(1800);
const inkOffline = await evaluate(INK);
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
console.log(`  在线画一笔：${JSON.stringify(online)}｜墨 ${inkOnline.ink}（分画布 ${JSON.stringify(inkOnline.parts)}）`);
console.log(`  离线画一笔：${JSON.stringify(offlineStroke)}｜墨 ${inkOffline.ink}（分画布 ${JSON.stringify(inkOffline.parts)}）`);
const grew = inkOffline.ink > inkOnline.ink;
const failures = [];
if (!online.ok) failures.push(`在线都没画上：${online.why}`);
if (inkOffline.canvases === 0) failures.push("页面里没有画布 ⇒ 判据无效");
if (!grew) failures.push(`离线这一笔没让墨量增加（${inkOnline.ink} ⇒ ${inkOffline.ink}）`);
socket.close();
if (failures.length) { console.log(`  ✗ 离线还画不了：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线也能画：断网后一笔仍然上墨");
process.exit(0);
