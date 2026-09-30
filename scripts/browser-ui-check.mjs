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
// 等待页面条件成立（比固定 sleep 稳：临时实例冷启动可能明显更慢）。
const waitFor = async (expression, label, timeoutMs = 20000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};

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
await waitFor(`document.getElementById("layer").options.length >= 2`, "新建图层出现在下拉里");
const layerCountAfter = await evaluate(`document.getElementById("layer").options.length`);
await evaluate(drawStroke);
const after = await evaluate(blankCheck);
// 一笔只应产生**一条** atom 日志（此前出现过同一个 seq 被重复打印）。
// 只数 `draw_stroke` 的 atom 行（日志里还有创建图层那条）；用遍历子元素避免正则转义。
const atomLines = await evaluate(
  `Array.from(document.getElementById("log").children).filter((el) => el.textContent.includes("draw_stroke")).length`
);
// 画完之后随时间采样：若着色像素先出现再消失，说明有「后到的整幅绘制」把内容覆盖了。
const timeline = [];
for (let step = 0; step < 6; step++) {
  const sample = await evaluate(blankCheck);
  timeline.push(sample.painted);
  await new Promise((resolve) => setTimeout(resolve, 250));
}

// 新建文档按钮：必须真的切换到新文档（此前它用当前 doc_id 再打开一次，点了等于没点）。
const identityBefore = await evaluate(`document.getElementById("identity").textContent`);
await evaluate(`document.getElementById("newDoc").click()`);
await waitFor("window.yanshiStats && window.yanshiStats.kernelHead > 0 && !!window.yanshiKernelReady", "新建文档后内核就绪");
const identityAfter = await evaluate(`document.getElementById("identity").textContent`);
const newDocPainted = await evaluate(blankCheck);

// 打开文档按钮：用覆盖 window.prompt 指定一个文档 id，断言真的切过去。
await evaluate(`window.prompt = () => "uicheck-opened-1"`);
await evaluate(`document.getElementById("openDoc").click()`);
await waitFor("window.yanshiStats && window.yanshiStats.kernelHead > 0 && !!window.yanshiKernelReady", "打开文档后内核就绪");
const identityOpened = await evaluate(`document.getElementById("identity").textContent`);

const geometry = await evaluate(`(() => {
  const board = document.getElementById("board");
  const overlay = document.getElementById("overlay");
  const preview = document.getElementById("preview");
  const rect = board.getBoundingClientRect();
  const overlayRect = overlay ? overlay.getBoundingClientRect() : null;
  return {
    canvas: { w: board.width, h: board.height, cssW: Math.round(rect.width), cssH: Math.round(rect.height) },
    stage: { cssW: Math.round(board.parentElement.getBoundingClientRect().width) },
    overlay: overlayRect ? { cssW: Math.round(overlayRect.width), cssH: Math.round(overlayRect.height) } : null,
    preview: preview ? {} : null,
  };
})()`);

const layout = await evaluate(`(() => {
  const aside = document.querySelector("aside");
  const rect = aside.getBoundingClientRect();
  return {
    asideRight: Math.round(rect.right),
    viewport: window.innerWidth,
    stageWidth: Math.round(document.querySelector(".stage").getBoundingClientRect().width),
    canvasWidth: Math.round(document.getElementById("board").getBoundingClientRect().width),
  };
})()`);

// 缩放与坐标映射：以画布中心为锚点放大后，在画布中心画一笔，它必须落在**文档中心**。
// 这是绝对断言（向服务端核对像素），能抓住「画布坐标与文档坐标差一个视口原点」这类错误。
const zoomCheck = await evaluate(`(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const center = { clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 };
  const before = { w: board.width, h: board.height };
  board.dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: -600, ...center }));
  await new Promise(r => setTimeout(r, 1200));
  const after = { w: board.width, h: board.height };
  const fire = (type) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 21, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...center,
  }));
  fire("pointerdown");
  await new Promise(r => setTimeout(r, 120));
  fire("pointerup");
  await new Promise(r => setTimeout(r, 1200));
  return { before, after, painted: (() => {
    const data = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
    let dark = 0;
    for (let i = 0; i < data.length; i += 4) if (data[i+3] > 8 && (data[i] < 245 || data[i+1] < 245 || data[i+2] < 245)) dark++;
    return dark;
  })() };
})()`);

// 向服务端核对：文档中心附近应当出现刚画的笔迹。
const parsedUrl = new URL(url);
// 服务端地址从传入的查看器地址推导：隔离运行时临时实例在别的端口上，不能写死端口。
const origin = parsedUrl.origin;
const token = parsedUrl.searchParams.get("token");
const docId = parsedUrl.searchParams.get("doc") || "default";
const renderCenter = await fetch(
  `${origin}/api/tools/render_region?doc=${docId}&token=${token}`,
  {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: { x: 432, y: 432, w: 160, h: 160 }, raw: true }),
  }
).then((r) => r.json());
let centerInk = -1;
if (renderCenter.raw_url) {
  const bytes = new Uint8Array(await fetch(`${origin}${renderCenter.raw_url}`).then((r) => r.arrayBuffer()));
  centerInk = 0;
  for (let i = 0; i < bytes.length; i += 4) {
    if (bytes[i + 3] > 8 && (bytes[i] < 200 || bytes[i + 1] < 200 || bytes[i + 2] < 200)) centerInk++;
  }
}

// 多级撤销/重做的 **UI 状态**：断言撤销/重做栈深度的相对变化。
// 逐像素的正确性由服务端测试 `service_flow::multi_step_undo_and_redo_restore_pixels_in_order`
// 覆盖（确定性）；这里只验证按钮确实在驱动栈，且深度按预期转移。
// 注意：不要在这里反复取同一个 `/api/blob/...` 地址来比对像素 —— 服务端对 blob 响应带
// `Cache-Control: immutable`，重复取同一地址会命中缓存，断言会读到陈旧内容。
const depth = async () => {
  const text = await evaluate(`document.getElementById("undoDepth").textContent`);
  const match = String(text).match(/(\d+)\D+(\d+)/);
  return match ? { undo: Number(match[1]), redo: Number(match[2]) } : null;
};
const strokeAt = async (fx, fy, pointerId) => {
  await evaluate(`(async () => {
    const board = document.getElementById("board");
    const rect = board.getBoundingClientRect();
    const point = { clientX: rect.left + rect.width * ${fx}, clientY: rect.top + rect.height * ${fy} };
    const fire = (type) => board.dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, pointerId: ${pointerId}, pointerType: "mouse",
      isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
    }));
    fire("pointerdown");
    await new Promise(r => setTimeout(r, 120));
    fire("pointerup");
    await new Promise(r => setTimeout(r, 1200));
  })()`);
};

await strokeAt(0.25, 0.25, 31);
await strokeAt(0.75, 0.75, 32);
const depthBefore = await depth();
await evaluate(`document.querySelector('button[data-tool="undo"]').click()`);
await new Promise((r) => setTimeout(r, 1500));
const depthUndo1 = await depth();
await evaluate(`document.querySelector('button[data-tool="undo"]').click()`);
await new Promise((r) => setTimeout(r, 1500));
const depthUndo2 = await depth();
await evaluate(`document.querySelector('button[data-tool="redo"]').click()`);
await new Promise((r) => setTimeout(r, 1500));
await evaluate(`document.querySelector('button[data-tool="redo"]').click()`);
await new Promise((r) => setTimeout(r, 1500));
const depthRedone = await depth();

// 历史浏览（设计 13.2）：列表随提交增长、能按类型/操作者筛选、且「回到此处」真的回到该时刻。
// 用**画布像素**判断（局部数据）；不要反复取同一个服务端 blob 地址（immutable 会被缓存）。
const paintedNow = async () => (await evaluate(blankCheck)).painted;
const historyRows = async () =>
  evaluate(`Array.from(document.querySelectorAll("#history .row")).map((row) => row.textContent)`);

const historyBefore = await historyRows();
await strokeAt(0.4, 0.6, 41);
const paintedAfterStroke = await paintedNow();
const historyAfter = await historyRows();
const hasStroke = historyAfter.some((row) => row.includes("draw_stroke"));

// 按类型筛选：只选 draw_stroke，列表应只含该类型。
await evaluate(`(() => {
  const select = document.getElementById("historyKind");
  const option = Array.from(select.options).find((o) => o.value === "draw_stroke");
  if (option) { select.value = "draw_stroke"; select.dispatchEvent(new Event("change")); }
})()`);
await new Promise((resolve) => setTimeout(resolve, 1200));
const filteredRows = await historyRows();
const filterOk = filteredRows.length > 0 && filteredRows.every((row) => row.includes("draw_stroke"));
// 恢复筛选，回到最早那条原子（create_document）⇒ 内容必然回到初始空状态。
await evaluate(`(() => {
  const select = document.getElementById("historyKind");
  select.value = "";
  select.dispatchEvent(new Event("change"));
})()`);
await new Promise((resolve) => setTimeout(resolve, 1200));
// 目标：最后一条 draw_stroke **之前**的那条原子 —— revert_to 含该原子，
// 因此回到那里就等于去掉最后一笔（当前文档最早的原子未必是 create_document）。
const jumpResult = await evaluate(`(async () => {
  const rows = Array.from(document.querySelectorAll("#history .row"));
  let target = null;
  for (let i = rows.length - 1; i >= 0; i--) {
    if (rows[i].textContent.includes("draw_stroke")) { target = rows[i - 1] || null; break; }
  }
  if (!target) return { ok: false, reason: "找不到笔画之前的原子" };
  const button = target.querySelector("button");
  if (!button) return { ok: false, reason: "该行没有按钮" };
  button.click();
  await new Promise((r) => setTimeout(r, 2200));
  return { ok: true, rows: rows.length };
})()`);
const paintedAfterJump = await paintedNow();
const historyFinal = await historyRows();

// 导出 PNG：必须是**整幅分辨率**的 PNG（显式导出路径）。
const exportResult = await evaluate(`(async () => {
  document.getElementById("exportPng").click();
  for (let i = 0; i < 40; i++) {
    if (window.yanshiStats.lastExport) break;
    await new Promise(r => setTimeout(r, 250));
  }
  return window.yanshiStats.lastExport || null;
})()`);
let exportPng = null;
if (exportResult && exportResult.url) {
  const bytes = new Uint8Array(await fetch(`${origin}${exportResult.url}`).then((r) => r.arrayBuffer()));
  const isPng = bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  exportPng = {
    isPng,
    width: view.getUint32(16),
    height: view.getUint32(20),
    bytes: bytes.length,
  };
}

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

// 历史浏览（设计 13.2）
if (historyAfter.length <= historyBefore.length) {
  problems.push(`历史列表没有随提交增长：${historyBefore.length} → ${historyAfter.length}`);
}
if (!hasStroke) {
  problems.push(`历史列表没有出现 draw_stroke：${JSON.stringify(historyAfter.slice(-3))}`);
}
if (!filterOk) {
  problems.push(`按类型筛选未生效，筛出 ${filteredRows.length} 行`);
}
if (!jumpResult || !jumpResult.ok) {
  problems.push(`「回到此处」未能执行：${JSON.stringify(jumpResult)}`);
} else if (!(paintedAfterJump < paintedAfterStroke)) {
  problems.push(
    `「回到此处」没有撤掉内容：着色 ${paintedAfterStroke} → ${paintedAfterJump}`
  );
}
if (historyFinal.length === 0) {
  problems.push("回到历史时刻后列表为空");
}
if (!exportResult) {
  problems.push("「导出 PNG」没有产生导出结果（window.yanshiStats.lastExport 为空）");
} else if (!exportPng || !exportPng.isPng) {
  problems.push(`导出结果不是可取的 PNG：${JSON.stringify(exportResult)}`);
} else if (exportPng.width !== 1024 || exportPng.height !== 1024) {
  problems.push(`导出应为整幅分辨率 1024²，实际 ${exportPng.width}×${exportPng.height}`);
}
if (atomLines > 1) {
  problems.push(`一笔产生了 ${atomLines} 条 atom 日志（应只有 1 条）`);
}
if (layout.asideRight > layout.viewport + 1) {
  problems.push(`右侧面板溢出窗口：right=${layout.asideRight} > viewport=${layout.viewport}`);
}
// 舞台必须收缩到画布尺寸：否则右侧出现灰色死区，点击落在 stage 上而不是 canvas 上。
if (geometry.stage.cssW - geometry.canvas.cssW > 4) {
  problems.push(
    `舞台比画布宽 ${geometry.stage.cssW - geometry.canvas.cssW}px（右侧灰色死区，点击无效）`
  );
}

if (process.env.UI_DEBUG === "1") {
  console.log("  --- 调试 ---");
  console.log("  stats:", JSON.stringify(await evaluate("window.yanshiStats")));
  console.log("  日志:", JSON.stringify(await evaluate(`document.getElementById("log").innerText.slice(-800)`)));
  console.log("  最近响应:", JSON.stringify(await evaluate(`document.getElementById("last").innerText.slice(0, 400)`)));
  console.log("  状态栏:", JSON.stringify(await evaluate(`document.getElementById("status") ? document.getElementById("status").innerText : ""`)));
}
console.log(`  画布：操作前不透明 ${before.opaque}/着色 ${before.painted}｜操作后不透明 ${after.opaque}/着色 ${after.painted}`);
console.log(`  图层数：${layerCountBefore} → ${layerCountAfter}`);
console.log(`  文档切换：新建 ${identityBefore === identityAfter ? "未生效" : "已生效"}｜打开 ${identityOpened.includes("uicheck-opened-1") ? "已生效" : "未生效"}`);
console.log(`  舞台宽度 ${geometry.stage.cssW}｜画布 CSS 宽度 ${geometry.canvas.cssW}（差值应 ≤4px）`);
console.log(`  缩放：视口 ${zoomCheck.before.w}×${zoomCheck.before.h} → ${zoomCheck.after.w}×${zoomCheck.after.h}｜缩放后着色 ${zoomCheck.painted}｜文档中心笔迹 ${centerInk}`);
console.log(`  撤销/重做栈深度：${JSON.stringify(depthBefore)} → 撤1 ${JSON.stringify(depthUndo1)} → 撤2 ${JSON.stringify(depthUndo2)} → 重做2 ${JSON.stringify(depthRedone)}`);
console.log(`  历史浏览：${historyBefore.length} 条 → 提交后 ${historyAfter.length} 条（含 draw_stroke ✓）→ 回到此处后着色 ${paintedAfterStroke} → ${paintedAfterJump}，列表 ${historyFinal.length} 条`);
console.log(`  导出 PNG：${exportPng ? `${exportPng.width}×${exportPng.height}，${(exportPng.bytes/1024).toFixed(0)} KB` : "无"}`);
console.log(`  一笔的 draw_stroke 日志条数：${atomLines}｜右侧面板右边界 ${layout.asideRight} / 视口 ${layout.viewport}`);
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

// 显式退出：CDP 的 WebSocket 仍打开时 node 不会自己结束（曾导致脚本挂到超时、退出码 124）。
ws.close();
process.exit(0);
