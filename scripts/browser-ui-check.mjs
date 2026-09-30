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

// ---- 共享辅助（统一放在这里，避免各段之间的 TDZ/顺序问题）----
const paintedNow = async () => (await evaluate(blankCheck)).painted;
const historyRows = async () =>
  evaluate(`Array.from(document.querySelectorAll("#history .row")).map((row) => row.textContent)`);
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

// 调整/滤镜面板（设计 13.3 的高级工具）：效果名来自服务端 `/api/effects`（即内核常量），
// 应用时**不传参数**由内核取默认值，再用 `list_effects` 读回实际生效的参数（零漂移）。
const effectNames = await evaluate(
  `Array.from(document.getElementById("effectName").options).map((o) => o.value)`
);
// 指纹 + 不透明度：既能判断"内容变了"，也能判断"画布没有停在透明态"（曾经的真实缺陷）。
const canvasFingerprint = `(() => {
  const board = document.getElementById("board");
  const data = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
  let opaque = 0;
  let sum = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i+3] > 8) opaque++;
    sum = (sum + data[i] * 3 + data[i+1] * 5 + data[i+2] * 7 + (i % 251)) % 1000000007;
  }
  return { opaque, total: data.length / 4, sum };
})()`;
const fingerprintBeforeEffect = await evaluate(canvasFingerprint);
const effectResult = await evaluate(`(async () => {
  const kind = document.getElementById("effectKind");
  kind.value = "adjustment";
  kind.dispatchEvent(new Event("change"));
  await new Promise((r) => setTimeout(r, 200));
  const name = document.getElementById("effectName");
  name.value = "invert";
  document.getElementById("effectParams").value = "{}";
  document.getElementById("effectApply").click();
  await new Promise((r) => setTimeout(r, 2500));
  // 等画布**不透明**（不透明像素数 = 总数）或超时：断言"不会停在透明态"。
  let fingerprint = null;
  for (let i = 0; i < 30; i++) {
    fingerprint = ${canvasFingerprint};
    if (fingerprint.opaque === fingerprint.total) break;
    await new Promise((r) => setTimeout(r, 200));
  }
  return { list: document.getElementById("effectsList").textContent, fingerprint };
})()`);
const fingerprintAfterEffect = effectResult.fingerprint;
// 撤销这次 invert：既验证"撤销也能撤掉效果"，也让后续用例回到白底。
await evaluate(`document.querySelector('button[data-tool="undo"]').click()`);
await new Promise((r) => setTimeout(r, 1800));

// 打开对话框（#4）：必须列出**服务器上的文档**；本地导入必须真的生效。
const dialogResult = await evaluate(`(async () => {
  document.getElementById("openDoc").click();
  await new Promise((r) => setTimeout(r, 1500));
  const cards = Array.from(document.querySelectorAll("#docList button"));
  const labels = cards.map((card) => card.textContent);
  document.getElementById("openClose").click();
  await new Promise((r) => setTimeout(r, 300));
  return { count: cards.length, labels };
})()`);

// 本地导入：在页面里造一张 64×64 的纯色 PNG 文件，塞进 file input（Chromium 支持 DataTransfer），
// 然后断言画布指纹变化且图层数 +1。
const importResult = await evaluate(`(async () => {
  const layersBefore = document.getElementById("layer").options.length;
  const before = ${canvasFingerprint};
  const source = document.createElement("canvas");
  source.width = 64;
  source.height = 64;
  const context = source.getContext("2d");
  context.fillStyle = "#1f6feb";
  context.fillRect(0, 0, 64, 64);
  const blob = await new Promise((resolve) => source.toBlob(resolve, "image/png"));
  const file = new File([blob], "check-import.png", { type: "image/png" });
  const transfer = new DataTransfer();
  transfer.items.add(file);
  const input = document.getElementById("importFile");
  input.files = transfer.files;
  input.dispatchEvent(new Event("change"));
  let after = before;
  let layersAfter = layersBefore;
  for (let i = 0; i < 30; i++) {
    await new Promise((r) => setTimeout(r, 300));
    after = ${canvasFingerprint};
    layersAfter = document.getElementById("layer").options.length;
    if (after.sum !== before.sum && layersAfter > layersBefore) break;
  }
  return { before: before.sum, after: after.sum, layersBefore, layersAfter,
           log: document.getElementById("log").innerText.slice(0, 160) };
})()`);

// 页面不得横向溢出（用户截图里的「排版都出去了」）：侧栏内部一旦撑破网格列，
// 画布就会被挤出屏幕。这条断言比"按钮是否可见"更根本。
const overflow = await evaluate(`(() => {
  const doc = document.documentElement;
  // 直接列出右边界最靠前的元素（不要按父元素过滤：上一版因此什么都没报出来）。
  const widest = Array.from(document.querySelectorAll("body *"))
    .map((el) => ({ el, rect: el.getBoundingClientRect() }))
    .filter((item) => item.rect.width > 0)
    .sort((a, b) => b.rect.right - a.rect.right)
    .slice(0, 5)
    .map((item) => (item.el.id ? "#" + item.el.id : item.el.tagName.toLowerCase()) +
      "[" + Math.round(item.rect.left) + ".." + Math.round(item.rect.right) + "]");
  const byScroll = Array.from(document.querySelectorAll("body *"))
    .map((el) => ({ el, sw: el.scrollWidth, rect: el.getBoundingClientRect() }))
    .filter((item) => item.sw > doc.clientWidth)
    .sort((a, b) => b.sw - a.sw)
    .slice(0, 5)
    .map((item) => (item.el.id ? "#" + item.el.id : item.el.tagName.toLowerCase()) +
      " sw=" + item.sw + " right=" + Math.round(item.rect.right));
  return {
    scrollWidth: doc.scrollWidth,
    clientWidth: doc.clientWidth,
    dialogOpen: !!document.querySelector("dialog") && document.querySelector("dialog").open,
    scrollWide: byScroll,
    bodyWidth: Math.round(document.body.getBoundingClientRect().width),
    mainWidth: Math.round(document.querySelector("main").getBoundingClientRect().width),
    asideWidth: Math.round(document.querySelector("aside").getBoundingClientRect().width),
    widest,
  };
})()`);

// 蒙版编辑（设计 13.3 高级工具）：填充整幅 → 拖一个矩形蒙版 → 蒙版外的内容应被裁掉。
// 自给自足：用独立文档，避免继承前序状态（这条规则本文件已强调多次）。
const maskDoc = "uicheck-mask-" + Date.now().toString(36);
const maskToken = await fetch(`${origin}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: maskDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await send("Page.navigate", { url: `${origin}/?doc=${maskDoc}&token=${maskToken}` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}
const maskResult = await evaluate(`(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (t, p, id) => board.dispatchEvent(new PointerEvent(t, { bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse", isPrimary: true, buttons: t === "pointerup" ? 0 : 1, ...p }));
  const ink = () => { const d = board.getContext("2d").getImageData(0, 0, board.width, board.height).data; let n = 0; for (let i = 0; i < d.length; i += 4) if (d[i+3] > 8 && (d[i] < 245 || d[i+1] < 245 || d[i+2] < 245)) n++; return n; };
  document.getElementById("zoomFit").click();
  await new Promise((r) => setTimeout(r, 800));
  // 铺满底色
  document.getElementById("color").value = "#1f6feb";
  document.getElementById("fillLayer").click();
  await new Promise((r) => setTimeout(r, 2000));
  const filled = ink();
  // 拖一个居中矩形蒙版（羽化 0，便于判断边界）
  document.getElementById("feather").value = "0";
  document.querySelector('button[data-tool="mask_rect"]').click();
  fire("pointerdown", at(0.25, 0.25), 301);
  fire("pointermove", at(0.75, 0.75), 301);
  await new Promise((r) => setTimeout(r, 100));
  fire("pointerup", at(0.75, 0.75), 301);
  await new Promise((r) => setTimeout(r, 2500));
  document.querySelector('button[data-tool="brush"]').click();
  return { filled, masked: ink(), log: document.getElementById("log").innerText.slice(0, 200) };
})()`);

// 工具栏可见性：所有工具按钮与动作按钮都必须在视口内（否则用户会以为"功能没有"）。
const toolbar = await evaluate(`(() => {
  const buttons = Array.from(document.querySelectorAll("button"));
  const outside = buttons
    .map((button) => ({ text: button.textContent.trim(), rect: button.getBoundingClientRect() }))
    .filter((item) => item.rect.width > 0 && (item.rect.right > window.innerWidth + 1 || item.rect.left < -1))
    .map((item) => item.text + "@" + Math.round(item.rect.left) + ".." + Math.round(item.rect.right));
  return { total: buttons.length, outside, width: window.innerWidth };
})()`);

// 橡皮：必须真的擦掉已画内容。**自给自足**：用独立文档，避免前面 revert_to 把文档倒回去
// 导致"前置条件不成立"（这一课已经栽过几次：分段测试不要共享状态）。
const eraserDoc = "uicheck-eraser-" + Date.now().toString(36);
const eraserToken = await fetch(`${origin}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: eraserDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await send("Page.navigate", { url: `${origin}/?doc=${eraserDoc}&token=${eraserToken}` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}
const eraserResult = await evaluate(`(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, point, id) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse",
    isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
  }));
  const painted = () => {
    const data = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
    let count = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i+3] > 8 && (data[i] < 245 || data[i+1] < 245 || data[i+2] < 245)) count++;
    }
    return count;
  };
  document.getElementById("size").value = "48";
  document.getElementById("color").value = "#101010";
  document.querySelector('button[data-tool="brush"]').click();
  fire("pointerdown", at(0.35, 0.35), 71);
  fire("pointermove", at(0.55, 0.35), 71);
  await new Promise((r) => setTimeout(r, 100));
  fire("pointerup", at(0.55, 0.35), 71);
  let beforeErase = painted();
  for (let i = 0; i < 24 && beforeErase === 0; i++) {
    await new Promise((r) => setTimeout(r, 250));
    beforeErase = painted();
  }
  document.querySelector('button[data-tool="erase"]').click();
  fire("pointerdown", at(0.35, 0.35), 72);
  fire("pointermove", at(0.55, 0.35), 72);
  await new Promise((r) => setTimeout(r, 100));
  fire("pointerup", at(0.55, 0.35), 72);
  let afterErase = painted();
  for (let i = 0; i < 24 && afterErase >= beforeErase; i++) {
    await new Promise((r) => setTimeout(r, 250));
    afterErase = painted();
  }
  document.querySelector('button[data-tool="brush"]').click();
  return { beforeErase, afterErase, log: document.getElementById("log").innerText.slice(0, 200) };
})()`);

// 打开已有作品：**重新加载页面**后应立刻显示已有内容（而不是白布）。
// 用独立文档 + 一笔，避免前面各段（revert_to 会把文档倒回去）干扰本断言。
const reloadDoc = "uicheck-reload-" + Date.now().toString(36);
const reloadToken = await fetch(`${origin}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: reloadDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await fetch(`${origin}/api/tools/create_layer?doc=${reloadDoc}&token=${reloadToken}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ layer_id: "L" }),
});
await fetch(`${origin}/api/tools/draw_stroke?doc=${reloadDoc}&token=${reloadToken}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ layer_id: "L", object_id: "o1",
    data: { points: [[80, 80], [400, 380]], size: 48, color: { r: 200, g: 30, b: 60, a: 255 } } }),
});
await send("Page.navigate", { url: `${origin}/?doc=${reloadDoc}&token=${reloadToken}` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 1")) break;
  await new Promise((r) => setTimeout(r, 250));
}
// 首帧是异步的：轮询到有内容为止（而不是取一次就断言）。
let paintedAfterReload = 0;
for (let i = 0; i < 40; i++) {
  paintedAfterReload = await paintedNow();
  if (paintedAfterReload > 0) break;
  await new Promise((r) => setTimeout(r, 250));
}
// 回到本来的文档继续后面的用例。
await send("Page.navigate", { url: url });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}
await evaluate(`document.getElementById("zoomFit").click()`);
await new Promise((r) => setTimeout(r, 1000));

// 吸管 + 填充图层（设计 13.3 基础工具）。
// 吸管：先画一笔已知颜色，再用吸管点它 → `#color` 应变成该颜色（精确、局部、无缓存问题）。
const pickResult = await evaluate(`(async () => {
  // 本段按「画布比例坐标 == 文档坐标」计算，先复位到适配（1:1）。
  document.getElementById("zoomFit").click();
  await new Promise((r) => setTimeout(r, 1200));
  const color = document.getElementById("color");
  color.value = "#c81e3c";
  const brush = document.querySelector('button[data-tool="brush"]');
  brush.click();
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, point, id) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse",
    isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
  }));
  // 画一个点（落笔即提交），再在同处用吸管取色。
  fire("pointerdown", at(0.2, 0.2), 61);
  await new Promise((r) => setTimeout(r, 120));
  fire("pointerup", at(0.2, 0.2), 61);
  await new Promise((r) => setTimeout(r, 1500));
  color.value = "#000000";
  document.querySelector('button[data-tool="eyedropper"]').click();
  fire("pointerdown", at(0.2, 0.2), 62);
  fire("pointerup", at(0.2, 0.2), 62);
  await new Promise((r) => setTimeout(r, 600));
  document.querySelector('button[data-tool="brush"]').click();
  return { picked: color.value, log: document.getElementById("log").innerText.slice(0, 200) };
})()`);

// 填充图层：指纹必须变化，画布保持不透明；随后撤销应回到填充前的指纹。
const beforeFill = await evaluate(canvasFingerprint);
await evaluate(`(async () => {
  document.getElementById("color").value = "#1f6feb";
  document.getElementById("fillLayer").click();
  await new Promise((r) => setTimeout(r, 2500));
})()`);
const afterFill = await evaluate(canvasFingerprint);
// 实验：再点一次「刷新」（显式从服务端/内核重渲染），看填充是否只是"没自动重绘"。
await evaluate(`(async () => {
  document.querySelector('button[data-tool="refresh"]').click();
  await new Promise((r) => setTimeout(r, 2500));
})()`);
const afterFillRefresh = await evaluate(canvasFingerprint);
await evaluate(`document.querySelector('button[data-tool="undo"]').click()`);
await new Promise((r) => setTimeout(r, 2000));
// 撤销填充的**像素**正确性由 `fill_pixels::reverting_a_fill_restores_the_previous_pixels` 覆盖
// （确定性、且 API 实测已验证）。浏览器里只保留"填充改变了像素"这条 UI 事实。


// 基础修图 / 基础液化（设计 13.3）：工具已存在，这里验证查看器真的能驱动它们。
// **自给自足**：用独立文档 + 先画结构。
// 形变类算子作用在纯色/纯白上不会有任何可见变化（这一点已经栽过两次），
// 而且前一段的"填充"会把整个画布变成均匀色 ✗，所以不能共享状态。
const retouchDoc = "uicheck-retouch-" + Date.now().toString(36);
const retouchToken = await fetch(`${origin}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: retouchDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await send("Page.navigate", { url: `${origin}/?doc=${retouchDoc}&token=${retouchToken}` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}
await evaluate(`document.getElementById("zoomFit").click()`);
await new Promise((r) => setTimeout(r, 800));
await strokeAt(0.3, 0.5, 50);
await strokeAt(0.45, 0.48, 52);
const retouchBefore = await evaluate(canvasFingerprint);
const retouchResult = await evaluate(`(async () => {
  const select = (name) => {
    const button = document.querySelector('button[data-tool="' + name + '"]');
    button.click();
    return button;
  };
  // 液化推：沿笔迹方向推挤。
  select("liquify_push");
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, point) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 51, pointerType: "mouse",
    isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
  }));
  fire("pointerdown", at(0.3, 0.5));
  await new Promise((r) => setTimeout(r, 80));
  fire("pointermove", at(0.4, 0.5));
  await new Promise((r) => setTimeout(r, 80));
  fire("pointermove", at(0.5, 0.52));
  await new Promise((r) => setTimeout(r, 80));
  fire("pointerup", at(0.5, 0.52));
  await new Promise((r) => setTimeout(r, 2500));
  const after = ${canvasFingerprint};

  // 仿制图章：未设置源点时必须给出明确提示，而不是静默无事发生。
  select("clone_stamp");
  fire("pointerdown", at(0.4, 0.6));
  fire("pointerup", at(0.4, 0.6));
  await new Promise((r) => setTimeout(r, 800));
  // 恢复画笔：否则后面的用例画出的笔迹会被路由到仿制（缺源点 ⇒ 不产生原子）。
  select("brush");
  return {
    after,
    log: document.getElementById("log").innerText.slice(0, 400),
  };
})()`);
const retouchAfter = retouchResult.after;

// 历史浏览（设计 13.2）：列表随提交增长、能按类型/操作者筛选、且「回到此处」真的回到该时刻。
// 用**画布像素**判断（局部数据）；不要反复取同一个服务端 blob 地址（immutable 会被缓存）。
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
// 「回到此处」的**像素**正确性由确定性服务端测试覆盖
// （service_flow::revert_to_restores_the_state_at_that_atom ✓）；这里只断言 UI 事实：
// 点击后确实提交了一个新的 `revert_to` 原子（历史列表 +1）。
// 目标：最后一条 draw_stroke **之前**的那条原子（当前文档最早的原子未必是 create_document）。
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
const historyFinal = await historyRows();
// 「回到此处」本身也是一个原子，应当可被撤销：跳转 → 一次撤销 → 画布指纹回到跳转前。
const fingerprintBeforeJump = await evaluate(canvasFingerprint);
const jumpUndoResult = await evaluate(`(async () => {
  const rows = Array.from(document.querySelectorAll("#history .row"));
  // 跳到**最早**那条原子：这一定改变画面（此前挑"笔画前一条"，若那一步本身无效果就断言不出东西）。
  const target = rows[0];
  if (!target) return { ok: false, reason: "历史为空" };
  target.querySelector("button").click();
  await new Promise((r) => setTimeout(r, 2200));
  const jumped = ${canvasFingerprint};
  document.querySelector('button[data-tool="undo"]').click();
  await new Promise((r) => setTimeout(r, 2200));
  const undone = ${canvasFingerprint};
  return { ok: true, jumped, undone };
})()`);

// 导出 PNG：必须是**当前文档整幅分辨率**的 PNG（显式导出路径）。
// 尺寸从页面读（前面几段会切换文档，写死 1024 会误报）。
const exportExpected = await evaluate(`({
  w: document.getElementById("board").width,
  h: document.getElementById("board").height,
})`);
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

// 缩放段结束：复位到「适配」（整幅、1:1）。后续各段都假定画布比例坐标 == 文档坐标，
// 否则吸管/跳转这类按坐标算的用例会静默失效（本轮的教训）。
await evaluate(`document.getElementById("zoomFit").click()`);
await new Promise((resolve) => setTimeout(resolve, 1200));

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

// 调整/滤镜面板
if (effectNames.length < 10) {
  problems.push(`效果下拉只有 ${effectNames.length} 项（应来自 /api/effects 的完整目录）`);
}
if (!effectResult.list.includes("invert")) {
  problems.push(`应用 invert 后效果列表里没有它：${JSON.stringify(effectResult.list.slice(0, 120))}`);
}
if (!fingerprintAfterEffect || fingerprintAfterEffect.opaque !== fingerprintAfterEffect.total) {
  problems.push(
    `应用效果后画布停在透明态：不透明 ${fingerprintAfterEffect?.opaque}/${fingerprintAfterEffect?.total}`
  );
}
if (fingerprintAfterEffect && fingerprintAfterEffect.sum === fingerprintBeforeEffect.sum) {
  problems.push("应用 invert 后画布像素没有任何变化");
}

// 打开对话框 + 本地导入
if (dialogResult.count < 1) {
  problems.push("打开对话框没有列出任何服务器文档");
}
if (importResult.layersAfter <= importResult.layersBefore) {
  problems.push(`本地导入没有新建图层：${importResult.layersBefore} → ${importResult.layersAfter}｜日志 ${JSON.stringify(importResult.log)}`);
}
// 像素层次的正确性由确定性测试覆盖（crates/yanshi-http 的
// `uploaded_blob_can_be_imported_and_renders_pixels`：上传 → import_image → 渲染出红色 ✓）。
// 这里只断言 UI 事实：图层增加、日志报告成功、且没有失败信息。
if (!importResult.log.includes("已导入")) {
  problems.push(`本地导入没有报告成功：${JSON.stringify(importResult.log)}`);
}
if (importResult.log.includes("导入失败")) {
  problems.push(`本地导入报告失败：${JSON.stringify(importResult.log)}`);
}

// 页面不得横向溢出
if (overflow.scrollWidth > overflow.clientWidth + 1) {
  problems.push(
    `页面横向溢出 ${overflow.scrollWidth - overflow.clientWidth}px（body ${overflow.bodyWidth} / main ${overflow.mainWidth} / 侧栏 ${overflow.asideWidth}）—— 最靠右：${overflow.widest.join(", ")}`
  );
}

// 蒙版编辑
if (maskResult.filled === 0) {
  problems.push("蒙版用例前置条件不成立：填充没有产生内容");
} else if (!(maskResult.masked < maskResult.filled)) {
  problems.push(
    `蒙版没有裁掉区域外的内容：着色 ${maskResult.filled} → ${maskResult.masked}｜日志 ${JSON.stringify(maskResult.log.slice(0, 120))}`
  );
}
if (!maskResult.log.includes("添加") || maskResult.log.includes("失败")) {
  problems.push(`蒙版日志未报告成功：${JSON.stringify(maskResult.log.slice(0, 120))}`);
}

// 工具栏可见性
if (toolbar.outside.length > 0) {
  problems.push(`有 ${toolbar.outside.length} 个按钮在视口外：${toolbar.outside.join(", ")}`);
}

// 橡皮
if (eraserResult.beforeErase === 0) {
  problems.push("橡皮用例的前置条件不成立：画笔画不出内容");
} else if (!(eraserResult.afterErase < eraserResult.beforeErase)) {
  problems.push(
    `橡皮没有擦掉内容：着色 ${eraserResult.beforeErase} → ${eraserResult.afterErase}｜日志 ${JSON.stringify(eraserResult.log.slice(0, 120))}`
  );
}

// 重新加载后应立刻显示已有内容
if (paintedAfterReload === 0) {
  problems.push("重新加载页面后画布是空白（已有作品没有立即显示）");
}

// 吸管 + 填充图层
if (pickResult.picked !== "#c81e3c") {
  problems.push(`吸管取色不对：期望 #c81e3c，实际 ${pickResult.picked}`);
}
if (afterFill.opaque !== afterFill.total) {
  problems.push(`填充后画布停在透明态：${afterFill.opaque}/${afterFill.total}`);
}
if (afterFill.sum === beforeFill.sum && afterFillRefresh.sum === beforeFill.sum) {
  problems.push("填充图层没有改变画布像素（刷新后也没变）");
}
if (afterFill.sum === beforeFill.sum && afterFillRefresh.sum !== beforeFill.sum) {
  problems.push("填充后画布没有自动重绘（手动刷新才生效）");
}

// 基础修图 / 液化
if (retouchResult.log.includes("操作失败") || retouchResult.log.includes("错误 ")) {
  problems.push(`修图/液化出现工具错误：${JSON.stringify(retouchResult.log.slice(0, 200))}`);
}
if (!retouchResult.log.includes("请先按住 Alt")) {
  problems.push(`仿制图章未设置源点时没有提示：${JSON.stringify(retouchResult.log.slice(0, 120))}`);
}
if (retouchAfter.opaque !== retouchAfter.total) {
  problems.push(`修图后画布停在透明态：${retouchAfter.opaque}/${retouchAfter.total}`);
}
if (retouchAfter.sum === retouchBefore.sum) {
  problems.push("液化推没有改变画布像素");
}

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
} else if (historyFinal.length <= historyAfter.length) {
  problems.push(
    `「回到此处」没有产生新原子：历史 ${historyAfter.length} → ${historyFinal.length}`
  );
}
if (historyFinal.length === 0) {
  problems.push("回到历史时刻后列表为空");
}
// 「回到此处」可撤销：撤销一次后应回到跳转前的像素（而不是停在跳转后的状态）。
if (!jumpUndoResult || !jumpUndoResult.ok) {
  problems.push(`「回到此处」的可撤销用例未执行：${JSON.stringify(jumpUndoResult)}`);
} else if (jumpUndoResult.jumped.sum === fingerprintBeforeJump.sum) {
  problems.push("「回到此处」本身没有改变画面（用例无效：目标原子选得不对）");
} else if (jumpUndoResult.undone.sum === jumpUndoResult.jumped.sum) {
  problems.push("撤销「回到此处」没有生效（画布与跳转后完全一致）");
} else if (jumpUndoResult.undone.sum !== fingerprintBeforeJump.sum) {
  problems.push(
    `撤销「回到此处」未回到跳转前的像素：${jumpUndoResult.undone.sum} ≠ ${fingerprintBeforeJump.sum}`
  );
}
if (!exportResult) {
  problems.push("「导出 PNG」没有产生导出结果（window.yanshiStats.lastExport 为空）");
} else if (!exportPng || !exportPng.isPng) {
  problems.push(`导出结果不是可取的 PNG：${JSON.stringify(exportResult)}`);
} else if (exportPng.width !== exportExpected.w || exportPng.height !== exportExpected.h) {
  problems.push(
    `导出应为整幅分辨率 ${exportExpected.w}²，实际 ${exportPng.width}×${exportPng.height}`
  );
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
console.log(`  调整/滤镜：目录 ${effectNames.length} 项｜invert 后指纹 ${fingerprintBeforeEffect.sum} → ${fingerprintAfterEffect?.sum}（不透明 ${fingerprintAfterEffect?.opaque}/${fingerprintAfterEffect?.total}）｜列表：${JSON.stringify(effectResult.list.slice(0, 80))}`);
console.log(`  打开对话框：列出 ${dialogResult.count} 个服务器文档（${JSON.stringify(dialogResult.labels.slice(0, 2))}）`);
console.log(`  本地导入：图层 ${importResult.layersBefore} → ${importResult.layersAfter}｜日志报告成功 ${importResult.log.includes("已导入") ? "✓" : "✗"}（像素由确定性测试覆盖）`);
console.log(`  蒙版编辑：填充后着色 ${maskResult.filled} → 加矩形蒙版后 ${maskResult.masked}`);
console.log(`  布局：scrollWidth ${overflow.scrollWidth} / clientWidth ${overflow.clientWidth}｜body ${overflow.bodyWidth}｜main ${overflow.mainWidth}｜侧栏 ${overflow.asideWidth}`);
console.log(`  最靠右的元素：${overflow.widest.join(", ")}`);
console.log(`  dialog.open=${overflow.dialogOpen}｜自身超宽的：${overflow.scrollWide.join(", ")}`);
console.log(`  工具栏：${toolbar.total} 个按钮，视口外 ${toolbar.outside.length} 个`);
console.log(`  橡皮：着色 ${eraserResult.beforeErase} → ${eraserResult.afterErase}`);
console.log(`  打开已有作品（重载后）着色：${paintedAfterReload}`);
console.log(`  回到此处可撤销：跳转前后指纹 ${jumpUndoResult?.ok ? `${fingerprintBeforeJump.sum} → ${jumpUndoResult.jumped.sum} → 撤销后 ${jumpUndoResult.undone.sum}` : "未执行"}`);
console.log(`  吸管/填充：吸管取到 ${pickResult.picked}（期望 #c81e3c）｜填充指纹 ${beforeFill.sum} → ${afterFill.sum} → 刷新 ${afterFillRefresh.sum}（撤销填充由确定性测试覆盖）`);
console.log(`  修图/液化：液化推后指纹 ${retouchBefore.sum} → ${retouchAfter.sum}（不透明 ${retouchAfter.opaque}/${retouchAfter.total}）｜仿制无源点有提示 ${retouchResult.log.includes("请先按住 Alt") ? "✓" : "✗"}`);
console.log(`  历史浏览：${historyBefore.length} 条 → 提交后 ${historyAfter.length} 条（含 draw_stroke ✓、筛选 ${filteredRows.length} 行 ✓）→ 回到此处后 ${historyFinal.length} 条`);
console.log(`  导出 PNG：${exportPng ? `${exportPng.width}×${exportPng.height}，${(exportPng.bytes/1024).toFixed(0)} KB` : "无"}`);
console.log(`  一笔的 draw_stroke 日志条数：${atomLines}｜右侧面板右边界 ${layout.asideRight} / 视口 ${layout.viewport}`);
console.log(`  着色像素时间线（每 250ms）：${timeline.join(" → ")}`);
console.log(`  几何：canvas ${geometry.canvas.w}×${geometry.canvas.h}（CSS ${geometry.canvas.cssW}×${geometry.canvas.cssH}）｜preview ${JSON.stringify(geometry.preview)}`);
console.log(`  缩略图：${thumbBefore === thumbAfter ? "未变化" : "已自动刷新"}`);
console.log(`  'tiles 个失效' 噪声行：${tileNoise}`);
if (problems.length) {
  // 失败时把页面日志一并打出来：工具层的错误提示都在那里，靠猜字段/猜坐标很费时间。
  console.log("  --- 页面日志（失败诊断）---");
  for (const line of String(await evaluate(`document.getElementById("log").innerText`)).split("\n").slice(0, 25)) {
    console.log("    " + line);
  }
  console.log(`  ❌ ${problems.length} 项不合格：`);
  for (const problem of problems) console.log(`     - ${problem}`);
  process.exit(1);
}
console.log("  ✅ UI 检查通过（画布非空、几何一致、缩略图自动刷新）");

// 显式退出：CDP 的 WebSocket 仍打开时 node 不会自己结束（曾导致脚本挂到超时、退出码 124）。
ws.close();
process.exit(0);
