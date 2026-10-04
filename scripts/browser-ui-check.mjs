#!/usr/bin/env node
// **采集日志时不要截断** ✓（第 359 轮实测 ✓）：原先这里取 `innerText.slice(0, 200)` ✗ ⇒
// 日志面板里**原子行排在前面** ✓ ⇒ 断言要找的「添加」**落在 200 字符之外** ✗ ⇒
// **断言永远看不到它** ✓（现象：`⚠ 蒙版日志未报告成功` ✓）。
// ⇒ 规则：**做判断的字符串要完整 ✓，给读者看的才截断** ✓（消息里仍用 slice ✓）。
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
// **按 URL 匹配目标** ✓ —— 此前取的是"第一个 page" ✗：长期运行的调试浏览器里
// 往往还开着别的页面（我自己实验留下的 ✓），于是检查跑在**旧页面**上 ✓，
// 现象是"点击完全没反应、断言全错" ✗（但页面本身是好的 ✓），排查代价极高 ✓。
const viewerBase = url.split("?")[0];
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(viewerBase)) ||
  list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
if (!target.url.startsWith(viewerBase)) {
  console.error(`注意：调试浏览器里没有正在打开 ${viewerBase} 的页面，将复用 ${target.url}`);
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



// 沿路径拖一整笔 ✓（介质整笔验收用 ✓）：页面内派发 pointerdown → 多次 pointermove → pointerup ✓。
const dragPath = async (points, pointerId) => {
  await evaluate(`(async () => {
    const board = document.getElementById("board");
    const rect = board.getBoundingClientRect();
    const at = (p) => ({ clientX: rect.left + rect.width * p[0], clientY: rect.top + rect.height * p[1] });
    const fire = (type, p) => board.dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, pointerId: ${pointerId}, pointerType: "mouse",
      isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...at(p),
    }));
    const path = ${JSON.stringify(points)};
    fire("pointerdown", path[0]);
    for (let i = 1; i < path.length; i++) {
      fire("pointermove", path[i]);
      await new Promise((r) => setTimeout(r, 80));
    }
    fire("pointerup", path[path.length - 1]);
  })()`);
};

const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    ?.result?.value;

// **截图** ✓：用户反复要求"用截图验收界面" ✓，而本脚本此前只能打文本断言 ✓
// ⇒ 排版类改动（两列、隐藏、全屏）**看得到才敢说做对了** ✓。
// 写到仓库外的临时目录 ✓（截图是**验证产物** ✓，不该进版本库 ✓）。
const shotsDir = "/tmp/yanshi-ui";
await (async () => { try { await import("node:fs/promises").then((fs) => fs.mkdir(shotsDir, { recursive: true })); } catch (_) {} })();
const capture = async (name) => {
  try {
    const shot = await send("Page.captureScreenshot", { format: "png" });
    // **`send` 返回的是原始 CDP 消息** ✓ ⇒ 数据在 `.result.data` ✓
    //（我第一版读 `.data` ✗ ⇒ 拿到 undefined ⇒ 打印"本机不支持截图" ✓ ——
    //  脚本自己的错被当成了浏览器能力不足 ✗，这类"把 bug 说成环境限制"最误导 ✓）。
    const data = shot?.result?.data ?? shot?.data;
    if (!data) return null;
    const fs = await import("node:fs/promises");
    const path = `${shotsDir}/${name}.png`;
    await fs.writeFile(path, Buffer.from(data, "base64"));
    return path;
  } catch (_) {
    return null;
  }
};

// **面板状态的干净基线** ✓ —— 加这一段的直接原因：面板可见性**持久化**在 localStorage 里 ✓，
// 而上一轮检查**失败退出**时把 `hide-rail` / `hide-dockers` 留在了那里 ✗
// ⇒ 下一轮点"隐藏"其实是在"显示" ✓ ⇒ 断言集体反向 ✓，还连累了**别段**的排版断言
//（"右侧面板应≥240px，实际 0px" ✗）—— 典型的**跨轮次状态泄漏假失败** ✓。
// 做法 ✓：导航后清掉该键 ✓ 并重载一次 ✓（在断言开始之前 ✓），保证每轮都从默认布局出发 ✓。
const resetPanelState = async () => {
  await evaluate(`(() => { try { localStorage.removeItem("yanshi.panels"); } catch (_) {} return true; })()`);
  await send("Page.reload", { ignoreCache: true });
  for (let i = 0; i < 80; i++) {
    if (await evaluate(`!!document.querySelector("#tools button")`)) return true;
    await new Promise((r) => setTimeout(r, 150));
  }
  return false;
};

await send("Runtime.enable");
await send("Page.enable");
// **禁用浏览器缓存** ✓ —— 加这一条的直接原因：调试浏览器缓存了**旧版 HTML** ✗，
// 于是页面里没有我刚加的折叠代码（点击无反应 ✓）、介质工具还是旧的**单点**逻辑 ✓
//（"包围盒宽 48" ✓）—— 而服务端与代码都是新的 ✓，症状极难归因 ✓。
await send("Network.enable", {});
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Page.navigate", { url });
// **把页面带到前台并启用焦点模拟** ✓ —— 加这一段的直接原因：**后台标签页**里
// `requestAnimationFrame` 不会回调 ✓（连 `setTimeout` 也被节流 ✓），
// 而查看器的服务端像素补画依赖它 ✓ ⇒ 检查会看到"画布全白"✗ 这类**环境性假失败** ✓
//（四位子 agent 独立撞到同一现象 ✓，他们都用 `Page.bringToFront` 绕开 ✓）。
await send("Page.bringToFront", {});
try {
  await send("Emulation.setFocusEmulationEnabled", { enabled: true });
} catch (_) { /* 老版本 Chromium 可能不支持 ✓ */ }
// **等页面脚本真正就绪再开始断言** ✓ —— 加这一段的直接原因：查看器的初始化在脚本后段
//（`initDockers()` 等 ✓），而检查导航后立刻开始交互 ✓，于是"点击折叠没反应" ✗、
// "介质工具还是旧的单点行为" ✗ 这类**竞态**会伪装成功能 bug ✓，极难归因 ✓。
// 判据取"工具条已渲染 + 关键初始化已完成" ✓，而不是固定 sleep ✓。
// **先复位持久化的面板状态** ✓（见 `resetPanelState` 的说明 ✓）——
// 必须在**开始断言之前**做 ✓，否则上一轮留下的 `hide-rail` 会让这一轮全线反向 ✓。
let panelBaselineReady = false;
for (let i = 0; i < 80; i++) {
  const ready = await evaluate(`(() => {
    try {
      return document.readyState === "complete" &&
        document.querySelectorAll("#tools button").length > 0 &&
        Boolean(document.getElementById("workspace")) &&
        [...document.querySelectorAll("aside .card h2")].length > 0;
    } catch (_) { return false; }
  })()`);
  if (ready) { panelBaselineReady = true; break; }
  await new Promise((r) => setTimeout(r, 250));
}
if (panelBaselineReady) {
  await resetPanelState();
}
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
    // 带上**中心点** ✓：画布是否在舞台里居中，靠比较两者的中心最直接 ✓
    //（只看宽度差看不出"贴左"还是"居中" ✓）。
    canvas: { w: board.width, h: board.height, cssW: Math.round(rect.width), cssH: Math.round(rect.height),
              centerX: rect.left + rect.width / 2, centerY: rect.top + rect.height / 2 },
    stage: (() => {
      const stageRect = board.parentElement.getBoundingClientRect();
      return { cssW: Math.round(stageRect.width), cssH: Math.round(stageRect.height),
               centerX: stageRect.left + stageRect.width / 2,
               centerY: stageRect.top + stageRect.height / 2 };
    })(),
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
  // **缩放改成显式入口** ✓（滚轮现在**只平移、不缩放** ✓ —— 用户实测"误触放大缩小"✗，
  // 所以判据必须跟着改 ✓：这里用状态栏的百分比输入框 ✓，仍能测"缩放后坐标映射还对不对" ✓）。
  const zoomBox = document.getElementById("zoomInput");
  zoomBox.value = "200";
  zoomBox.dispatchEvent(new Event("change", { bubbles: true }));
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
// **`problems` 必须在最前面声明** ✓ —— 后面多处断言（布局结构、工具条、介质…）都引用它 ✓；
// 曾经它声明在很后面 ✗，于是一旦那些断言真的触发就会抛 "Cannot access 'problems' before initialization" ✓，
// 把"断言失败"变成"脚本崩溃" ✗ —— 排查代价远高于失败本身 ✓。
const problems = [];
const parsedUrl = new URL(url);
// 服务端地址从传入的查看器地址推导：隔离运行时临时实例在别的端口上，不能写死端口。
const origin = parsedUrl.origin;
const token = parsedUrl.searchParams.get("token");
const docId = parsedUrl.searchParams.get("doc") || "default";
// **示例作品入口** ✓ —— 用户应当能直接打开示例查看 ✓（见 docs/samples.md ✓）。
// 只断言**入口存在且可读** ✓，**不点击** ✗：点击会走"打开或创建" ✓，
// 若示例尚未画出来就会被**建成空文档** ✗（污染工作区 ✓，本会话已避免这一坑 ✓）。
// **不打开对话框** ✗ —— 第一版打开/关闭了 `#openDialog` ✓，而对话框是 `showModal` ✓
// ⇒ 布局变化 ⇒ `sizeBoards` **清空画布** ✓ ⇒ 紧随其后的介质用例量到的是**白板** ✗
//（三位子 agent 报的"打开/刷新后画布全白"与这条同源 ✓ —— 本会话第二次栽在
// "断言本身改变了被测状态"上 ✓）。改为**读定义**验证入口 ✓，不碰布局 ✓。
const sampleAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  const box = document.getElementById("sampleList");
  const table = (typeof SAMPLES !== "undefined" && SAMPLES) || [];
  return {
    tableCount: table.length,
    entries: table.map((s) => s.id),
    labels: table.map((s) => s.label),
    hasContainer: Boolean(box),
    renderer: typeof renderSamples === "function",
  };
})())`));
if (!sampleAudit || sampleAudit.tableCount < 4) {
  problems.push(`示例作品定义不足（${sampleAudit ? sampleAudit.tableCount : "?"} 个，应 ≥4）`);
} else if (!sampleAudit.hasContainer || !sampleAudit.renderer) {
  problems.push(`示例入口未接线：容器 ${sampleAudit.hasContainer}｜渲染函数 ${sampleAudit.renderer}`);
} else if (sampleAudit.labels.some((label) => !label || label.length < 2)) {
  problems.push(`示例入口缺少可读名称：${JSON.stringify(sampleAudit.labels)}`);
} else {
  console.log(`  示例作品：${sampleAudit.tableCount} 个定义｜${sampleAudit.entries.join(" / ")}`);
}

// **页面内联脚本的语法检查** ✓ —— 加这一段的直接原因：我拼接代码时在顶层留了一段
// `await ...` ✗ ⇒ 整页脚本 SyntaxError ⇒ **整个界面白掉** ✓（工具条 0 个按钮 ✓），
// 而当时的检查只报"等待超时" ✗，症状离病因很远 ✓。
// 这里用 `new Function` 只做**语法**校验（不执行 ✓）⇒ 这类问题立刻可见 ✓。
const pageHtml = await fetch(url).then((r) => r.text());
const inlineScript = pageHtml.split("<script>")[1] ? pageHtml.split("<script>")[1].split("</script>")[0] : "";
let inlineScriptSyntax = "ok";
try {
  // eslint-disable-next-line no-new-func
  new Function(inlineScript);
} catch (error) {
  inlineScriptSyntax = String(error && error.message ? error.message : error);
}
if (inlineScriptSyntax !== "ok") {
  problems.push(`页面内联脚本存在语法错误（界面会整页失效）：${inlineScriptSyntax}`);
}
console.log(`  页面脚本：${inlineScript.length} 字节｜语法 ${inlineScriptSyntax === "ok" ? "ok ✓" : "错误 ✗ " + inlineScriptSyntax}`);

const renderCenter = await fetch(
  `${origin}/api/tools/render_region?doc=${docId}&token=${token}`,
  {
    method: "POST",
    headers: { "content-type": "application/json" },
    // **区域要落在文档里** ✓（第 455 轮 ✓）：原来写的是 432,432,160,160 ✗ ——
    // 那是**为 1024² 画布写的中心** ✓（引入它的提交 6d55973 就叫 1024-square canvas ✓），
    // 而 runner 建的文档是 **320×240**（run-criteria.sh:52 ✓）⇒ 432..592 **整个在文档外** ✗
    // ⇒ 取回的是空内容 ✓。这里改用**左上角一块**：**对任何不小于 160 的文档都成立** ✓，
    // **不写死 320×240** ✗（写死就是同一个错误的第三次 ✓）。
    body: JSON.stringify({ region: { x: 0, y: 0, w: 160, h: 160 }, raw: true }),
  }
).then((r) => r.json());
// **这里原本只算不断言** ✗（第 463 轮 ✓）：centerInk 被赋值三次而**没有任何地方用它** ✓
// ⇒ 这一段**永远不会失败** ✓ ⇒ 属于「静默检查」那一族 ✓。
// 补的断言**只声明「这条路能取回像素」** ✓ —— 因为「文档里该有多少内容」取决于用例 ✓，
// 断言它反而是替这段代码猜意图 ✗；而「取回 raw_url」正是这一段在做的事 ✓，且**它能失败** ✓。
if (!renderCenter || !renderCenter.raw_url) {
  problems.push(
    `文档左上一块取不回像素（raw_url 缺失）⇒ 区域渲染这条路断了｜返回 ${JSON.stringify(renderCenter).slice(0, 120)}`,
  );
}
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
await send("Page.navigate", { url: `${origin}/?doc=${maskDoc}&token=${maskToken}&debug=1` });
for (let i = 0; i < 80; i++) {
  // **就绪条件要含"段首要用的元素"** ✓（第 592 轮 ✓）：原先只等 kernelHead > 0 ✗
  // ⇒ 而段首紧接着用 zoomFit / color / data-tool="brush" / size / feather ✓
  // ⇒ 内核就绪与 DOM 齐了是两件事 ✓ ⇒ 元素未到时第一处 click 就抛 ✗
  // ⇒ 整段 reject ⇒ 外部拿到一串 undefined ✓（第 589 轮实测：着色 undefined → undefined ✓）。
  if (await evaluate(`window.yanshiStats && window.yanshiStats.kernelHead > 0
      && document.getElementById("board") && document.getElementById("zoomFit")
      && document.getElementById("color") && document.getElementById("size")
      && document.getElementById("feather")
      && document.querySelector('button[data-tool="brush"]')`)) break;
  await new Promise((r) => setTimeout(r, 250));
}
const maskResult = await evaluate(`(async () => {
  // **段内保护** ✓（第 599 轮 ✓）：段首若某个元素取不到，第一处 click/value 就抛 ✗
  // ⇒ 整段 reject ⇒ 外部拿到一串 undefined ✓（实测：着色 undefined -> undefined、日志 "" ✓）
  // ⇒ 那样报的是"判据失败"，而真相是"判据无法运行" ✓ —— 两者必须分开 ✓。
  const missing = [];
  for (const id of ["board", "zoomFit", "color", "size", "feather"]) {
    if (!document.getElementById(id)) missing.push(id);
  }
  if (!document.querySelector('button[data-tool="brush"]')) missing.push("brush 按钮");
  if (missing.length) {
    return { unable: "缺元素：" + missing.join("、"), filled: undefined, masked: undefined, log: "" };
  }
  // **整段套 try** ✓（第 609 轮 ✓）：段内某处抛错 ⇒ 整段 reject ⇒ 外部只见 undefined ✗
  // ⇒ 把消息与阶段带回来 ✓（**一处 try 省掉 N 轮猜测** ✓）。
  try {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (t, p, id) => board.dispatchEvent(new PointerEvent(t, { bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse", isPrimary: true, buttons: t === "pointerup" ? 0 : 1, ...p }));
  const ink = () => { const d = board.getContext("2d").getImageData(0, 0, board.width, board.height).data; let n = 0; for (let i = 0; i < d.length; i += 4) if (d[i+3] > 8 && (d[i] < 245 || d[i+1] < 245 || d[i+2] < 245)) n++; return n; };
  document.getElementById("zoomFit").click();
  await new Promise((r) => setTimeout(r, 800));
  // 铺底：**用画笔铺**而不是 heavy 填充 ✓ ——
  // 填充是 heavy 原子，其异步渲染落地时间不稳定，本段曾多次误报"填充没有产生内容" ✗
  //（选区/文本段改用画笔铺底后就没再出现 ✓，这里沿用同一做法 ✓）。
  document.getElementById("color").value = "#1f6feb";
  document.querySelector('button[data-tool="brush"]').click();
  document.getElementById("size").value = "200";
  for (let row = 0; row < 4; row++) {
    const y = 0.15 + row * 0.23;
    fire("pointerdown", at(0.02, y), 640 + row);
    fire("pointermove", at(0.5, y), 640 + row);
    fire("pointermove", at(0.98, y), 640 + row);
    await new Promise((r) => setTimeout(r, 60));
    fire("pointerup", at(0.98, y), 640 + row);
    await new Promise((r) => setTimeout(r, 300));
  }
  let filled = ink();
  for (let i = 0; i < 120 && filled === 0; i++) {
    await new Promise((r) => setTimeout(r, 250));
    filled = ink();
  }
  // 拖一个居中矩形蒙版（羽化 0，便于判断边界）
  document.getElementById("feather").value = "0";
  document.querySelector('button[data-tool="mask_rect"]').click();
  // **点完按钮，等工具真的切过去再按下** ✓（第 474 轮 ✓）：原先点击之后**立刻**发 pointerdown ✗
  // ⇒ 若切换慢一拍 ✓ ⇒ **按下时工具还不是 mask_rect** ✗ ⇒ **state.points 拿不到点** ✓
  // ⇒ 产品报「蒙版需要拖出一个区域」✓ ⇒ ⇒ **表现为「间歇性」** ✗（第 473 轮实测：这一项时有时无 ✓）。
  // ⇒ **等条件，不等时长** ✓ —— 与第 450 轮那个稳定性等待同一手法 ✓。
  // 注意：**本段处在模板字符串里** ⇒ 注释里不能出现反引号 ✗（本轮我已因此失败一次 ✓）。
  let toolAfterClick = null;
  for (let i = 0; i < 20; i++) {
    toolAfterClick = window.yanshi && window.yanshi.state ? window.yanshi.state().tool : "(没有 state())";
    if (toolAfterClick === "mask_rect") break;
    await new Promise((r) => setTimeout(r, 100));
  }
  fire("pointerdown", at(0.25, 0.25), 301);
  // **派发之间让出一拍** ✓（第 576 轮 ✓）：实测同一个探针在两轮里读出 points=2 ✓ 与 points=0 ✗
  // ⇒ **这是竞态** ✓（不是逻辑错 ✓ —— 我为它读了六处产品代码 ✓、排除五个假设 ✗）
  // ⇒ 三次派发挤在同一个任务里 ⇒ 产品的处理有时还没就绪 ✓ ⇒ 这里各让 30ms ✓。
  await new Promise((r) => setTimeout(r, 30));
  // **中间要再 move 一次**（第 362 轮对照出来的）：上面画笔铺底是「0.02 → 0.5 → 0.98」
  // **两次 move**，而这里原先只有一次 ⇒ 产品报「蒙版需要拖出一个区域」⇒ **判据侧缺陷**，
  // 照它自己的正确写法补即可。（本行在模板字符串里 ⇒ 注释中不可出现反引号 ✗）
  fire("pointermove", at(0.5, 0.5), 301);
  // **第一次 move 之后，产品记了几个点** ✓（第 572 轮 ✓）：静态阅读已到极限 ✗
  // ⇒ 在两次 move 之间各读一次 ✓ ⇒ 就能分辨"第一次没记"还是"第二次把它清了" ✓。
  const ptsAfterMove1 = (window.yanshi && window.yanshi.state ? window.yanshi.state().points : "?");
  await new Promise((r) => setTimeout(r, 30));
  fire("pointermove", at(0.75, 0.75), 301);
  const ptsAfterMove2 = (window.yanshi && window.yanshi.state ? window.yanshi.state().points : "?");
  await new Promise((r) => setTimeout(r, 30));
  await new Promise((r) => setTimeout(r, 100));
  // **探针必须在抬手之前取** ✓（第 467 轮 ✓）：原先它写在 pointerup **之后** ✗
  // ⇒ 抓到的是"抬手之后"的日志 ✓ ⇒ **名字承诺了它没做的事** ✗（与"算了不断言"同族 ✓）。
  // **直接观测，不再猜上游** ✓（第 506 轮 ✓）：八个假设全被推翻之后 ✗，改成"读运行时的状态" ✓ ——
  // 判据早就能读 state() ✓（第 474 轮我自己加的 ✓）⇒ **能观测的，就别推理** ✓。
  // 观测值接进 logDuringDrag ✓ ⇒ **它会顺着已有的打印进日志** ✓，不必再改打印处 ✓。
  const stateProbe = window.yanshi && window.yanshi.state ? window.yanshi.state() : null;
  const logDuringDrag = document.getElementById("log").innerText.slice(-160)
    + "｜拖动中 tool=" + (stateProbe ? stateProbe.tool : "?")
    // 第 557 轮：state 的 points 已经是「点数」本身，我原先又取了一次 length
    // ⇒ 对数字 2 取 length 得 undefined（本地验证过）⇒ 日志打出 points=undefined
    // ⇒ 我据此以为「点没入列」，白读六处产品代码。现在直接打它：
    // 拖动中若为 2，就说明 commitMask 要求的「至少 2 个点」是满足的。
    + " points=" + (stateProbe ? stateProbe.points : "?")
    + " dragging=" + (stateProbe ? stateProbe.dragging : "?");
    + " ｜move1后=" + String(ptsAfterMove1) + " move2后=" + String(ptsAfterMove2)
    + " 抬手前=" + String(ptsBeforeUp);
  // **抬手之前**再读一次点数 ✓（第 577 轮 ✓）：实测拖动期是 2 ✓ 而蒙版仍失败 ✗
  // ⇒ 与 commitMask 的「< 2」矛盾 ✓ ⇒ 唯一解释是"抬手前掉了" ✓ ⇒ 这一读分辨它 ✓。
  const ptsBeforeUp = (window.yanshi && window.yanshi.state ? window.yanshi.state().points : "?");
  fire("pointerup", at(0.75, 0.75), 301);
  let masked = ink();
  for (let i = 0; i < 120 && masked >= filled; i++) {
    await new Promise((r) => setTimeout(r, 250));
    masked = ink();
  }
  document.querySelector('button[data-tool="brush"]').click();
  return { filled, masked, logDuringDrag, toolAfterClick, log: document.getElementById("log").innerText };
  } catch (err) {
    return { unable: "段内抛错：" + String((err && err.message) || err), filled: undefined, masked: undefined, log: , toolAfterClick: undefined };
  }
})()`);

// 移动工具（设计 13.3「移动」）：画一个矩形 → 用移动工具拖已知位移 →
// 通过 list_objects 断言 bbox **恰好**平移该位移（绝对量断言，不依赖指纹）。
const moveDoc = "uicheck-move-" + Date.now().toString(36);
const moveToken = await fetch(`${origin}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: moveDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await send("Page.navigate", { url: `${origin}/?doc=${moveDoc}&token=${moveToken}&debug=1` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}
const moveResult = await evaluate(`(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (t, p, id) => board.dispatchEvent(new PointerEvent(t, { bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse", isPrimary: true, buttons: t === "pointerup" ? 0 : 1, ...p }));
  document.getElementById("zoomFit").click();
  await new Promise((r) => setTimeout(r, 800));
  // 画一个矩形（0.2,0.2 → 0.4,0.4）
  document.querySelector('button[data-tool="rect"]').click();
  fire("pointerdown", at(0.2, 0.2), 401);
  fire("pointermove", at(0.4, 0.4), 401);
  await new Promise((r) => setTimeout(r, 100));
  fire("pointerup", at(0.4, 0.4), 401);
  await new Promise((r) => setTimeout(r, 2000));
  const before = await window.yanshiCallTool("list_objects", {});
  // 拖到 +0.2,+0.1（以画布比例折算成文档像素）
  document.querySelector('button[data-tool="move_object"]').click();
  fire("pointerdown", at(0.3, 0.3), 402);
  await new Promise((r) => setTimeout(r, 400));
  fire("pointermove", at(0.5, 0.4), 402);
  await new Promise((r) => setTimeout(r, 120));
  fire("pointerup", at(0.5, 0.4), 402);
  await new Promise((r) => setTimeout(r, 2200));
  const after = await window.yanshiCallTool("list_objects", {});
  const beforeBbox = (before.objects || []).map((o) => o.bbox).filter(Boolean)[0];
  const afterBbox = (after.objects || []).map((o) => o.bbox).filter(Boolean)[0];
  // 旧位置的像素断言放到 Node 侧（用 render_region 直接量 ✓）——
  // 第一版我在页面里猜视图变量名（viewState 等 ✗）去换算画布坐标，既脆弱又不可靠 ✓。
  return { beforeBbox, afterBbox, log: document.getElementById("log").innerText };
})()`);

// **移动后旧位置必须被清掉** ✓ —— 用户实测：移动后画布旧位置不刷新、留下残影 ✗
//（缩略图整幅重绘所以正常 ✓）。根因是脏区规划只用**新**包围盒 ✗（`dirty_for_object` ✓），
// 本用例因此直接量"旧包围盒里还有没有非背景像素" ✓，而不是只看对象属性 ✓。
if (moveResult && moveResult.beforeBbox) {
  const [bx, by, bw, bh] = moveResult.beforeBbox;
  const region = { x: bx, y: by, w: Math.max(1, bw), h: Math.max(1, bh) };
  // **必须用"当下活动的" doc/token** ✓（第 234 轮更正 ✓）：
  // 这一段前面（606 行 ✓）已经把页面切到 `moveDoc` ✓，而 `docId`/`token`（344/345 ✓）是**初始 URL** 的 ✓
  // ⇒ 旧版拿**另一份文档**去渲染 ✓ ⇒ 量到的 576 个"残影"像素其实是**那份文档自己的内容** ✗
  // ⇒ **产品那侧一直是好的** ✓（`dirty.rs` 已经做了新旧包围盒**并集** ✓、单测
  // `moving_an_object_clears_the_pixels_it_left_behind` 绿 ✓）⇒ **错的是这条探针** ✓。
  const rendered = await fetch(
    `${origin}/api/tools/render_region?doc=${moveDoc}&token=${moveToken}`,
    { method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ region, raw: true }) },
  ).then((r) => r.json()).catch(() => ({}));
  let leftover = -1;
  if (rendered.raw_url) {
    const bytes = new Uint8Array(await fetch(`${origin}${rendered.raw_url}`).then((r) => r.arrayBuffer()));
    let n = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      if (bytes[i + 3] > 8 && (bytes[i] < 245 || bytes[i + 1] < 245 || bytes[i + 2] < 245)) n += 1;
    }
    leftover = n;
  }
  console.log(`  移动旧位置：${leftover} 个非背景像素（须 0）｜旧包围盒 ${bx},${by},${bw},${bh}`);
  if (leftover > 0) {
    problems.push(`移动后旧位置仍有 ${leftover} 个非背景像素（画布残影 ✗）`);
  }
}

// 新建（用户要求：**给用户输入名字的机会**）与另存为副本。
const namedNew = "uicheck-named-" + Date.now().toString(36);
const copyName = namedNew + "-copy";
const namingResult = await evaluate(`(async () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  // ① 新建：点「新建」→ 对话框应出现并**预填**一个名字 → 改成我们指定的名字 → 创建。
  document.getElementById("newDoc").click();
  await wait(400);
  const dialog = document.getElementById("newDialog");
  const prefilled = document.getElementById("newName").value;
  document.getElementById("newName").value = ${JSON.stringify(namedNew)};
  document.getElementById("newCreate").click();
  for (let i = 0; i < 60; i++) {
    await wait(300);
    if (document.getElementById("identity").textContent.includes(${JSON.stringify(namedNew)})) break;
  }
  const identity = document.getElementById("identity").textContent;
  // 画点内容，便于比较副本。
  document.getElementById("zoomFit").click();
  await wait(700);
  document.getElementById("color").value = "#c81e3c";
  document.getElementById("size").value = "40";
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (t, p, id) => board.dispatchEvent(new PointerEvent(t, { bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse", isPrimary: true, buttons: t === "pointerup" ? 0 : 1, ...p }));
  fire("pointerdown", at(0.4, 0.4), 501);
  fire("pointermove", at(0.6, 0.5), 501);
  await wait(90);
  fire("pointerup", at(0.6, 0.5), 501);
  // **等「尺寸稳定」，而不是等「等于某个值」** ✓（第 450 轮 ✓）：原先等的是
  // board.width 等于 state.docSize.w ✗ —— 而 state.docSize **可能不存在** ✗ ⇒ 比较退化成
  // board.width 等于 -1 ⇒ **永远不为真** ⇒ 循环空转完 40 次 ⇒ **两次读数取在当时的任意布局尺寸上** ✓
  //（实测一次 512²、一次 1024² ✓ ⇒ total 不同只是症状 ✓，病因是守卫没生效 ✗）。
  // 注意：**本段处在模板字符串里** ⇒ 注释里**不能出现反引号** ✗（那会把模板闭合 ✓）。
  let before = ${canvasFingerprint};
  let beforeWidth = -1;
  let beforeStable = 0;
  for (let i = 0; i < 40; i++) {
    const width = document.getElementById("board").width;
    if (width > 0 && width === beforeWidth) beforeStable += 1; else beforeStable = 0;
    beforeWidth = width;
    if (before.sum !== 0 && beforeStable >= 2) break;
    await wait(250);
    before = ${canvasFingerprint};
  }

  // ② 另存为副本：打开对话框 → 填新名字 → 另存为… → 应切到副本且内容一致。
  document.getElementById("openDoc").click();
  await wait(1200);
  document.getElementById("copyName").value = ${JSON.stringify(copyName)};
  document.getElementById("copyDoc").click();
  for (let i = 0; i < 60; i++) {
    await wait(300);
    if (document.getElementById("identity").textContent.includes(${JSON.stringify(copyName)})) break;
  }
  let after = ${canvasFingerprint};
  let afterWidth = -1;
  let afterStable = 0;
  for (let i = 0; i < 40; i++) {
    const width = document.getElementById("board").width;
    if (width > 0 && width === afterWidth) afterStable += 1; else afterStable = 0;
    afterWidth = width;
    if (after.sum !== 0 && afterStable >= 2) break;
    await wait(250);
    after = ${canvasFingerprint};
  }
  return {
    prefilled,
    identity,
    before: before.sum,
    after: after.sum,
    // 尺寸也带出来（第 253 轮）：sum 是一个**取模哈希**，它同时依赖像素值与 data.length，
    // 所以两块画布尺寸不同时，内容再一样也会不同。断言必须先看 total 是否相同。
    beforeTotal: before.total,
    afterTotal: after.total,
    log: document.getElementById("log").innerText,
  };
})()`);

// 选区（路线 A：约束落笔）与文本工具。
// 规范写法（前两轮失败后重写）：单次求值 + **阶段标记** ⇒ 出错时能精确指认在哪一步 ✗；
// 页面侧一律 `try/catch` 返回 `{error, stage}` ⇒ 不会退化成 undefined 让 Node 报谜语错误 ✓。
// 判据按**笔画颜色**数 ✓（乐观预览与内核渲染有 LSB 差异 ✗）；结论前**等画布稳定** ✓。
const selDoc = "uicheck-sel-" + Date.now().toString(36);
const selToken = await fetch(`${origin}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: selDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await send("Page.navigate", { url: `${origin}/?doc=${selDoc}&token=${selToken}` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}
const selectionResult = await evaluate(`(async () => {
  let stage = "init";
  try {
    const wait = (ms) => new Promise((r) => setTimeout(r, ms));
    const board = document.getElementById("board");
    const snapshot = () => Array.from(board.getContext("2d").getImageData(0, 0, board.width, board.height).data);
    const stable = async () => {
      let previous = snapshot();
      for (let i = 0; i < 40; i++) {
        await wait(250);
        const current = snapshot();
        let same = true;
        for (let index = 0; index < current.length; index += 97 * 4) {
          if (current[index] !== previous[index]) { same = false; break; }
        }
        previous = current;
        if (same && i >= 2) return current;
      }
      return previous;
    };

    stage = "几何";
    const rect = board.getBoundingClientRect();
    const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
    const fire = (t, p, id) => board.dispatchEvent(new PointerEvent(t, {
      bubbles: true, cancelable: true, pointerId: id, pointerType: "mouse",
      isPrimary: true, buttons: t === "pointerup" ? 0 : 1, ...p,
    }));
    document.getElementById("zoomFit").click();
    await wait(800);

    stage = "铺底";
    document.getElementById("color").value = "#1f6feb";
    document.querySelector('button[data-tool="brush"]').click();
    document.getElementById("size").value = "200";
    for (let row = 0; row < 5; row++) {
      const y = 0.1 + row * 0.2;
      fire("pointerdown", at(0.02, y), 610 + row);
      fire("pointermove", at(0.5, y), 610 + row);
      fire("pointermove", at(0.98, y), 610 + row);
      await wait(60);
      fire("pointerup", at(0.98, y), 610 + row);
      await wait(300);
    }
    await stable();

    stage = "建选区";
    const selectButton = document.querySelector('button[data-tool="select_rect"]');
    if (!selectButton) throw new Error("工具栏里没有选区按钮");
    document.getElementById("feather").value = "0";
    selectButton.click();
    fire("pointerdown", at(0.25, 0.25), 620);
    fire("pointermove", at(0.75, 0.75), 620);
    await wait(120);
    fire("pointerup", at(0.75, 0.75), 620);
    await wait(1200);

    stage = "画笔触";
    document.getElementById("color").value = "#ffd166";
    document.querySelector('button[data-tool="brush"]').click();
    document.getElementById("size").value = "40";
    fire("pointerdown", at(0.05, 0.5), 621);
    fire("pointermove", at(0.5, 0.5), 621);
    fire("pointermove", at(0.95, 0.5), 621);
    await wait(120);
    fire("pointerup", at(0.95, 0.5), 621);
    const after = await stable();

    stage = "量选区";
    const width = board.width;
    const height = board.height;
    const strokeColor = (data, index) => data[index] > 200 && data[index + 1] > 170 && data[index + 2] < 140;
    const inSelection = (x, y) =>
      x >= width * 0.25 - 2 && x <= width * 0.75 + 2 && y >= height * 0.25 - 2 && y <= height * 0.75 + 2;
    let inside = 0;
    let outside = 0;
    for (let index = 0; index < after.length; index += 4) {
      if (!strokeColor(after, index)) continue;
      const x = (index / 4) % width;
      const y = Math.floor((index / 4) / width);
      if (inSelection(x, y)) inside += 1; else outside += 1;
    }

    stage = "清选区";
    const clearButton = document.getElementById("clearSelection");
    if (!clearButton) throw new Error("工具栏里没有清除选区按钮");
    clearButton.click();
    await new Promise((r) => setTimeout(r, 900));
    // **清除之后必须**再画一笔，否则 "clearedOutside" 必然为 0（第 301 轮更正）：
    // 清除选区**不可能**让先前被裁掉的墨"追溯地"出现 —— 那些像素从来没被画上去。
    // 旧版清完就直接测量 ⇒ 断言永远不成立（**判据自己错了**，不是产品）。
    fire("pointerdown", at(0.05, 0.75), 622);
    fire("pointermove", at(0.5, 0.75), 622);
    fire("pointermove", at(0.95, 0.75), 622);
    await new Promise((r) => setTimeout(r, 80));
    fire("pointerup", at(0.95, 0.75), 622);
    await stable();

    const cleared = await stable();
    let clearedOutside = 0;
    for (let index = 0; index < cleared.length; index += 4) {
      if (!strokeColor(cleared, index)) continue;
      const x = (index / 4) % width;
      const y = Math.floor((index / 4) / width);
      if (!inSelection(x, y)) clearedOutside += 1;
    }

    stage = "文本";
    const textButton = document.querySelector('button[data-tool="text"]');
    if (!textButton) throw new Error("工具栏里没有文本按钮");
    // 用 **CJK** 验收：这条路径会走内嵌 OFL 图集 ✓（纯 ASCII 走内置 5×7 ✓）。
    window.prompt = () => "中文永";
    const beforeText = await stable();
    textButton.click();
    fire("pointerdown", at(0.3, 0.12), 622);
    const afterText = await stable();
    let textChanged = 0;
    for (let index = 0; index < afterText.length; index += 4) {
      if (afterText[index] !== beforeText[index] || afterText[index + 1] !== beforeText[index + 1]) {
        textChanged += 1;
      }
    }
    document.querySelector('button[data-tool="brush"]').click();

    const log = document.getElementById("log").innerText;
    return {
      ok: true, inside, outside, clearedOutside, textChanged,
      created: log.includes("已创建选区"),
      cleared: log.includes("已清除选区"),
      textLogged: log.includes("已输入文本"),
    };
  } catch (error) {
    return { error: String((error && error.message) || error), stage };
  }
})()`);

// 可折叠 Dockers + 工作区预设 ✓（借鉴成熟绘画软件的面板/工作区 ✓）。
// 断言三件事 ✓：折叠真的改变了可见高度 ✓、工作区预设真的切换折叠集合 ✓、状态被持久化 ✓。
const dockerAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  const cards = [...document.querySelectorAll("aside .card")];
  const byTitle = (t) => cards.find((c) => (c.querySelector("h2") || {}).textContent.trim() === t);
  const height = (card) => (card ? Math.round(card.getBoundingClientRect().height) : -1);
  const collapsed = (card) => Boolean(card && card.classList.contains("collapsed"));
  const effects = byTitle("调整 / 滤镜");
  const stateBefore = collapsed(effects);
  const before = {
    cards: cards.length,
    collapsed: cards.filter(collapsed).map((c) => c.querySelector("h2").textContent.trim()),
    effectsHeight: height(effects),
    effectsCollapsed: stateBefore,
  };
  // 点标题 ⇒ 状态应**翻转** ✓（默认"绘画"工作区里它本来是折叠的 ✓，
  // 所以这一下是**展开** ✓ —— 第一版我写死了"应当折叠" ✗，方向反了 ✓）。
  if (effects) effects.querySelector("h2").click();
  const afterCollapse = {
    collapsed: collapsed(effects),
    effectsHeight: height(effects),
    flipped: collapsed(effects) !== stateBefore,
    grew: height(effects) > before.effectsHeight,
  };
  // 切到"校对"工作区 ✓。
  const select = document.getElementById("workspace");
  if (select) { select.value = "review"; select.dispatchEvent(new Event("change", { bubbles: true })); }
  const afterWorkspace = {
    collapsed: cards.filter(collapsed).map((c) => c.querySelector("h2").textContent.trim()),
    effectsCollapsed: collapsed(byTitle("调整 / 滤镜")),
    historyCollapsed: collapsed(byTitle("历史（原子日志）")),
    stored: localStorage.getItem("yanshi.workspace"),
    storedDockers: localStorage.getItem("yanshi.dockers"),
  };
  return { before, afterCollapse, afterWorkspace, cards: cards.length };
})())`));
if (dockerAudit.cards < 6) {
  problems.push(`Dockers 数量异常：${dockerAudit.cards}（应 ≥6）`);
}
if (!dockerAudit.afterCollapse.flipped) {
  problems.push("点击 Dockers 标题没有切换折叠状态");
}
if (!dockerAudit.afterCollapse.grew === dockerAudit.before.effectsCollapsed) {
  // 折叠 → 展开 应**变高** ✓；展开 → 折叠 应**变矮** ✓。
  problems.push(`折叠切换后的可见高度变化方向不对（${dockerAudit.before.effectsHeight} → ${dockerAudit.afterCollapse.effectsHeight}）`);
}
if (dockerAudit.afterWorkspace.stored !== "review") {
  problems.push(`工作区选择没有持久化（localStorage=${dockerAudit.afterWorkspace.stored}）`);
}
// "校对"预设：历史与日志**展开** ✓、调整/内核/缩略图**折叠** ✓。
if (dockerAudit.afterWorkspace.historyCollapsed || !dockerAudit.afterWorkspace.effectsCollapsed) {
  problems.push(`"校对"工作区的折叠集合不对：${JSON.stringify(dockerAudit.afterWorkspace.collapsed)}`);
}
// **"绘画"预设** ✓ —— 加这条断言的直接原因：截图核验时发现"绘画"工作区里
// 「WASM 计算内核」卡片竟然**折叠**着 ✗，而预设明确把它列为 open ✓，
// 根因是**标题逐字匹配**（全角括号/空格差异 ⇒ 匹配静默失败 ✓）。
const paintPreset = JSON.parse(await evaluate(`JSON.stringify((() => {
  const select = document.getElementById("workspace");
  select.value = "paint";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  const cards = [...document.querySelectorAll("aside .card")];
  const isCollapsed = (title) => {
    const card = cards.find((c) => (c.querySelector("h2") || {}).textContent.includes(title));
    return card ? card.classList.contains("collapsed") : null;
  };
  return { kernel: isCollapsed("计算内核"), history: isCollapsed("历史"), effects: isCollapsed("调整") };
})())`));
if (paintPreset.kernel !== false || paintPreset.history !== true || paintPreset.effects !== true) {
  problems.push(`"绘画"工作区的开合不对（内核应展开、历史与调整应折叠）：${JSON.stringify(paintPreset)}`);
}
console.log(`  "绘画"工作区：内核 ${paintPreset.kernel === false ? "展开 ✓" : "折叠 ✗"}｜历史 ${paintPreset.history ? "折叠 ✓" : "展开 ✗"}｜调整 ${paintPreset.effects ? "折叠 ✓" : "展开 ✗"}`);


console.log(`  Dockers：${dockerAudit.cards} 个面板｜折叠切换高度 ${dockerAudit.before.effectsHeight} → ${dockerAudit.afterCollapse.effectsHeight}` +
  `｜"校对"折叠 ${dockerAudit.afterWorkspace.collapsed.length} 个｜持久化 ${dockerAudit.afterWorkspace.stored}`);

// 布局结构断言 ✓ —— 加这一段的直接原因：我在"移动选项栏"时把 <nav id="tools"> 的
// 开闭标签一起删掉了 ✗，20 个按钮于是变成 main 的网格子元素、三列铺满整页 ✓，
// 而当时的检查只测"按钮是否在视口内" ✓ 竟然全部通过 ✗（布局错了却全绿 ✓）。
// 教训：检查必须断言**结构**（谁是子元素、每列多宽），不能只看可见性 ✓。
const layoutStructure = JSON.parse(await evaluate(`JSON.stringify((() => {
  const nav = document.querySelector("main > nav#tools");
  const aside = document.querySelector("main > aside");
  const stage = document.querySelector("main > .stage");
  const options = document.getElementById("options");
  return {
    navExists: Boolean(nav),
    navWidth: nav ? Math.round(nav.getBoundingClientRect().width) : -1,
    asideWidth: aside ? Math.round(aside.getBoundingClientRect().width) : -1,
    stageWidth: stage ? Math.round(stage.getBoundingClientRect().width) : -1,
    optionsInMain: Boolean(document.querySelector("main > #options")),
    optionsAboveMain: Boolean(options) && options.getBoundingClientRect().bottom <=
      document.querySelector("main").getBoundingClientRect().top + 1,
    buttons: document.querySelectorAll("#tools button").length,
  };
})())`));
if (!layoutStructure) {
  problems.push("布局结构断言失败：拿不到结构数据");
} else {
  if (!layoutStructure.navExists) problems.push("布局结构错误：main 下没有 nav#tools（工具按钮的包裹元素丢了？）");
  if (layoutStructure.navWidth > 80) problems.push("工具条应窄（≤80px），实际 " + layoutStructure.navWidth + "px");
  if (layoutStructure.buttons < 15) problems.push("工具条内按钮过少：" + layoutStructure.buttons);
  if (layoutStructure.asideWidth < 240) problems.push("右侧面板应≥240px，实际 " + layoutStructure.asideWidth + "px");
  if (layoutStructure.stageWidth < 200) problems.push("画布区应≥200px，实际 " + layoutStructure.stageWidth + "px");
  if (layoutStructure.optionsInMain) problems.push("选项栏不应作为 main 的网格子元素（会占用首列）");
  if (!layoutStructure.optionsAboveMain) problems.push("选项栏应在 main 之上（整宽一行）");
  console.log("  布局结构：工具条 " + layoutStructure.navWidth + "px（" + layoutStructure.buttons + " 个按钮）｜画布 " +
    layoutStructure.stageWidth + "px｜面板 " + layoutStructure.asideWidth + "px");
}

// 工具条图标与提示 ✓（界面第②步）：每个工具按钮都必须有**图标**与**可读提示** ✓，
// 且快捷键提示要有意义 ✓ —— 只断言"按钮存在"是不够的 ✗（本次截图化改造就是证据：按钮一直在 ✓，
// 但之前挤成中文折行 ✓）。另外顺带确认 `data-tool` 仍保留 ✓ ⇒ 其余用例的选取器不受影响 ✓。
const iconAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  const buttons = [...document.querySelectorAll("#tools button")];
  return {
    total: buttons.length,
    withIcon: buttons.filter((b) => b.querySelector("svg")).length,
    withHint: buttons.filter((b) => (b.getAttribute("title") || "").length >= 2).length,
    withToolAttr: buttons.filter((b) => b.dataset.tool).length,
    withId: buttons.filter((b) => b.id).length,
    named: buttons.filter((b) => (b.getAttribute("aria-label") || "").length >= 2).length,
  };
})())`));
if (iconAudit.withIcon !== iconAudit.total) {
  problems.push(`工具条有 ${iconAudit.total - iconAudit.withIcon} 个按钮没有图标`);
}
if (iconAudit.withHint !== iconAudit.total) {
  problems.push(`工具条有 ${iconAudit.total - iconAudit.withHint} 个按钮缺少悬停提示`);
}
if (iconAudit.named !== iconAudit.total) {
  problems.push(`工具条有 ${iconAudit.total - iconAudit.named} 个按钮缺少无障碍名称`);
}
if (iconAudit.withToolAttr + iconAudit.withId !== iconAudit.total) {
  problems.push("工具条按钮既没有 data-tool 也没有 id（选取器会失效）");
}
console.log(`  工具条：${iconAudit.total} 个按钮｜图标 ${iconAudit.withIcon}｜提示 ${iconAudit.withHint}｜无障碍名称 ${iconAudit.named}`);

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
  return { beforeErase, afterErase, log: document.getElementById("log").innerText };
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
// **自给自足**：用自己的新文档（前面「新建/另存为」段会把页面切到副本，
// 共享状态会让本段读到别的文档内容 —— 这一课本文件已记录多次）。
const pickDoc = "uicheck-pick-" + Date.now().toString(36);
const pickToken = await fetch(`${origin}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: pickDoc, width: 512, height: 512 }),
}).then((r) => r.json()).then((v) => v.token);
await send("Page.navigate", { url: `${origin}/?doc=${pickDoc}&token=${pickToken}` });
for (let i = 0; i < 80; i++) {
  if (await evaluate("window.yanshiStats && window.yanshiStats.kernelHead > 0")) break;
  await new Promise((r) => setTimeout(r, 250));
}

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
  return { picked: color.value, log: document.getElementById("log").innerText };
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
// **必须在跳转之前取** ✓（第 477 轮 ✓）：原先这一行写在下面那次跳转**之后** ✗
// ⇒ 页面若在等待的 2.2s 内跳完 ⇒ 它取到的其实是「跳转后」的画面 ✓
// ⇒ 于是 `:1793` 那条判定（jumped 与 beforeJump 相等）**必然成立** ✓
// ⇒ 判据便报「没有改变画面（用例无效：目标原子选得不对）」✓ —— **而那是它的猜测** ✗
// ⇒ ⇒ **真正的错是"取样时机"** ✓，**而第 458 轮我照它的猜测改了目标选取** ✗（改了它猜的那件事 ✓）。
// ⇒ 这也是"时有时无"的来源 ✓：**取决于那 2.2s 内跳完没跳完** ✓。
// **先画一笔，让"跳转前"非空** ✓（第 493 轮 ✓）：原先这一段**指望前面几段留下的画面** ✗
// ⇒ 而那几段会切文档／清空 ✓ ⇒ 跳转前本来就是初始态 ✗ ⇒ **"跳到最早那条原子"一个像素都不改** ✓
// ⇒ `jumped === fingerprintBeforeJump` ⇒ 判据报「目标原子选得不对」✗（**而那是它的猜测** ✓）。
// ⇒ 所以这里**自己把状态造出来** ✓，**不依赖别人** ✗ —— 段落之间的隐式依赖，一改上游就静默失效 ✓。
await strokeAt(0.5, 0.5, 41);
const fingerprintBeforeJump = await evaluate(canvasFingerprint);
const jumpResult = await evaluate(`(async () => {
  const rows = Array.from(document.querySelectorAll("#history .row"));
  // **挑「足够靠前」的目标** ✓（第 458 轮 ✓）：原先是从最后往前找 draw_stroke 行再取**它的上一行** ✗
  // ⇒ 那一行往往与「跳转前」同态 ✗ ⇒ 跳过去画面不变 ⇒ 判据自己报
  // **「回到此处」本身没有改变画面（用例无效：目标原子选得不对）** ✓（**它说得对** ✓）。
  // ⇒ 改成挑**第一个带按钮的行** ✓：它在**任何一笔之前** ✓ ⇒ **跳过去画面必然不同** ✓。
  // 注意：**本段处在模板字符串里** ⇒ 注释里**不能出现反引号** ✗（这一轮我又踩了一次 ✓）。
  let target = rows.find((row) => row.querySelector("button")) || null;
  if (!target) return { ok: false, reason: "找不到可跳转的原子行" };
  const button = target.querySelector("button");
  if (!button) return { ok: false, reason: "该行没有按钮" };
  button.click();
  // **等条件，不等时长** ✓（第 587 轮 ✓）：原先固定等 2200ms ✗ ⇒ 实测这条断言**间歇失败** ✓
  // ⇒ 与蒙版同源 ✓（两处都靠定时等待 ✓）⇒ 这里改成"等历史条数稳定" ✓（连续两次相同即认为跳完 ✓），
  // 上限约 6 秒 ✓（超时后照常返回，让断言自己说话 ✓，不在这里硬造通过 ✗）。
  let prevLen = -1;
  let stableTicks = 0;
  for (let i = 0; i < 60; i++) {
    const len = document.querySelectorAll("#history .row").length;
    if (len === prevLen) stableTicks += 1; else stableTicks = 0;
    prevLen = len;
    if (stableTicks >= 2) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  return { ok: true, rows: rows.length };
})()`);
const historyFinal = await historyRows();
// **跳转是一次导航，不是一次编辑** ✓（设计 13.2：历史的数据源是原子日志 ✓、支持"按原子步进" ✓，
// **没有**"回到此处会产生新原子"这一条 ✗ —— 那是判据作者自己加的 ✓）。
// 本段仍检查"跳转后画面确实回到那一刻" ✓（那是导航该有的效果 ✓），
// **但不再要求它进历史** ✓；撤销那一路也据此**不假定**多了一个原子 ✓。

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
// **必须"在页面里"读字节** ✓（第 231 轮更正 ✓）：
// 导出现在是**本地优先** ✓（`board.toBlob()` ⇒ `blob:` URL ✓）⇒ 而 `blob:` 是**页面作用域**的 ✗
// ⇒ **node 根本 fetch 不了它** ✗（旧版写 `fetch(\`${origin}${url}\`)` ✓ ⇒ 拼出非法 URL ✓ ⇒ 判据**崩溃** ✓，
// 实测：`Failed to parse URL from http://127.0.0.1:…/blob:http://…` ✗）。
// 搬进页面之后**两种模式都成立** ✓：`blob:`（本地导出 ✓）与 `/api/…`（服务端回退 ✓）在页面里都 fetch 得动 ✓。
const exportPng = await evaluate(`(async () => {
  const info = window.yanshiStats.lastExport || null;
  if (!info || !info.url) return null;
  const bytes = new Uint8Array(await (await fetch(info.url)).arrayBuffer());
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    isPng: bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47,
    width: view.getUint32(16),
    height: view.getUint32(20),
    bytes: bytes.length,
    via: String(info.url).startsWith("blob:") ? "blob" : "http",
  };
})()`);

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
if (!(importResult.log || "").includes("已导入")) {
  problems.push(`本地导入没有报告成功：${JSON.stringify(importResult.log)}`);
}
if ((importResult.log || "").includes("导入失败")) {
  problems.push(`本地导入报告失败：${JSON.stringify(importResult.log)}`);
}

// 页面不得横向溢出
if (overflow.scrollWidth > overflow.clientWidth + 1) {
  problems.push(
    `页面横向溢出 ${overflow.scrollWidth - overflow.clientWidth}px（body ${overflow.bodyWidth} / main ${overflow.mainWidth} / 侧栏 ${overflow.asideWidth}）—— 最靠右：${overflow.widest.join(", ")}`
  );
}

// 蒙版编辑
// Node 侧守卫（第 620 轮）：我先前把 try/catch 加在页面内，而失败发生在 CDP/Node 层，
// 也就是 await evaluate 自己 reject：maskResult 会是 undefined，
// 于是下一行读它的 filled 就抛 TypeError —— 实测堆栈正是 browser-ui-check.mjs:1531:16，
// 而它被记成「蒙版没有裁掉区域外」，把「判据无法运行」当成了「判据失败」。
if (!maskResult) {
  problems.push("蒙版判据无法运行：页面侧求值没有返回结果（不是判据失败）");
} else if (maskResult.filled === 0) {
  problems.push("蒙版用例前置条件不成立：填充没有产生内容");
} else if (!(maskResult.masked < maskResult.filled)) {
  problems.push(
    `蒙版没有裁掉区域外的内容：着色 ${maskResult.filled} → ${maskResult.masked}｜日志 ${JSON.stringify((maskResult.log || "").slice(0, 120))}`
  );
}
if (!(maskResult.log || "").includes("添加") || (maskResult.log || "").includes("失败")) {
  problems.push(`蒙版日志未报告成功：${JSON.stringify((maskResult.log || "").slice(0, 120))}`);
}

// 移动工具
if (!moveResult.beforeBbox || !moveResult.afterBbox) {
  problems.push(`移动用例缺少 bbox：${JSON.stringify(moveResult)}`);
} else {
  const dx = Math.round(moveResult.afterBbox[0] - moveResult.beforeBbox[0]);
  const dy = Math.round(moveResult.afterBbox[1] - moveResult.beforeBbox[1]);
  // 期望位移 ≈ 画布 0.2/0.1 比例 × 文档边长（512）。
  const wantX = Math.round(512 * 0.2);
  const wantY = Math.round(512 * 0.1);
  if (Math.abs(dx - wantX) > 2 || Math.abs(dy - wantY) > 2) {
    problems.push(
      `移动位移不对：期望 ≈(${wantX}, ${wantY})，实际 (${dx}, ${dy})｜bbox ${JSON.stringify(moveResult.beforeBbox)} → ${JSON.stringify(moveResult.afterBbox)}`
    );
  }
  if (!(moveResult.log || "").includes("已移动")) {
    problems.push(`移动日志未报告成功：${JSON.stringify((moveResult.log || "").slice(0, 120))}`);
  }
}

// 新建（可输入名字）与另存为副本
if (!namingResult.prefilled) {
  problems.push("「新建」对话框没有预填名字（用户要求：新建时能输入名字）");
}
if (!namingResult.identity.includes(namedNew)) {
  problems.push(`「新建」没有切到用户输入的名字：identity=${JSON.stringify(namingResult.identity)}`);
}
if (namingResult.before === 0) {
  problems.push("新建文档里画不出内容（前置条件不成立）");
} else if (namingResult.beforeTotal !== namingResult.afterTotal) {
  // **先把"尺寸不同"与"内容不同"分开**（第 253 轮）：sum 是哈希，尺寸变了它必然变，
  // 拿它断言"副本内容不对"是把两件事混成一件（本项第 5 次同类）。
  problems.push(
    `另存为副本的画布尺寸不同（${namingResult.beforeTotal} vs ${namingResult.afterTotal} 像素）⇒ sum 不可比｜日志 ${JSON.stringify((namingResult.log || "").slice(0, 120))}`
  );
} else if (namingResult.after !== namingResult.before) {
  problems.push(
    `另存为副本内容不一致（同尺寸 ${namingResult.beforeTotal} 像素）：源 ${namingResult.before} vs 副本 ${namingResult.after}｜日志 ${JSON.stringify((namingResult.log || "").slice(0, 120))}`
  );
}

// 选区与文本
if (!selectionResult || !selectionResult.ok) {

  problems.push(`选区/文本用例失败：${JSON.stringify(selectionResult)}`);
} else {
  if (!selectionResult.created || !selectionResult.cleared) {
    problems.push(`选区创建/清除日志缺失：${JSON.stringify(selectionResult)}`);
  }
  if (selectionResult.inside === 0) {
    problems.push("选区用例前置条件不成立：选区内没有笔画色");
  }
  if (selectionResult.outside > 0) {
    problems.push(`选区外出现了笔画色：越界 ${selectionResult.outside} 个像素`);
  }
  if (selectionResult.clearedOutside === 0) {
    problems.push("清除选区后选区外仍看不到笔画色（应恢复为不受约束）");
  }
  if (!selectionResult.textLogged) {
    problems.push(`文本工具日志缺失：${JSON.stringify(selectionResult)}`);
  }
  if (selectionResult.textChanged === 0) {
    problems.push("文本工具没有画出任何像素");
  }
}

// 介质插件（设计 11.1）：宿主加载 wasm 插件 → 产出像素 → 入 CAS 与日志 ✓。
//
// 分工照本脚本的惯例 ✓：Node 侧派发输入并轮询 API 与状态，页面只回报像素。
// 失败时把 **window.yanshiStats.medium** 与查看器日志一起打出来 ✓ ——
// 上一轮卡住的原因就是"没有可观测信号" ✗，这次先要信号再定位 ✓。
const MEDIUM_INK = `(() => {
  const d = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
  let n = 0;
  for (let i = 0; i < d.length; i += 4) if (d[i+3] > 8 && (d[i] < 245 || d[i+1] < 245 || d[i+2] < 245)) n++;
  return n;
})()`;
// **记住当前图层并在本段结束后恢复** ✓ —— 介质落笔会新建图层并切过去 ✓，
// 后面（蒙版等）段落默认"当前图层"是自己的 ✓，不恢复就会互相干扰 ✓
//（本会话真实踩过：整笔落墨变多后，蒙版段当场失败 ✗）。
const mediumLayerBefore = await evaluate(`state.layerId`);
const mediumBefore = await evaluate(MEDIUM_INK) || 0;
await evaluate(`document.querySelector('button[data-tool="medium_dab"]').click()`);
// **拖一整笔** ✓（此前只点一下 ✓）：断言"真的是笔触"而不只是"落了一个点" ✓。
await dragPath([[0.2, 0.7], [0.35, 0.62], [0.5, 0.72], [0.65, 0.62], [0.8, 0.7]], 501);
// 先取查看器当前文档（它可能已经是某个副本 ✓）。
const mediumPage = (await evaluate(`({ docId: window.yanshiStats.docId, token: window.yanshiStats.token })`)) || {};
const mediumDoc = mediumPage.docId || reloadDoc;
const mediumToken = mediumPage.token || reloadToken;
let mediumObject = null;
let mediumStatus = null;
let mediumRawListing = null;
for (let i = 0; i < 40 && !mediumObject; i++) {
  await new Promise((r) => setTimeout(r, 250));
  mediumStatus = await evaluate(`window.yanshiStats && window.yanshiStats.medium || null`);
  if (mediumStatus && (mediumStatus.status === "error" || mediumStatus.status === "rejected")) break;
  // 用**查看器此刻真正打开的文档** ✓ —— 页面把它暴露在 yanshiStats.docId/token 上 ✓。
  // 本段前两版分别用了重载前的 docId ✗ 与 reloadDoc ✗，都因后面的"新建/另存为"段
  // 已经把页面切到别的文档而查错对象 ✓（实测对象列表里只剩 o1 ✓）。
  const listed = await fetch(
    `${origin}/api/tools/list_objects?doc=${mediumDoc}&token=${mediumToken}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: "{}" },
  ).then((response) => response.json()).catch(() => ({}));
  mediumRawListing = listed;
  mediumObject = (listed.objects || []).find((o) => o.medium && o.medium.id) || null;
}
// **对"画布有墨"本身轮询** ✓ —— 此前是"轮询对象存在 ✓ 然后**只量一次**" ✗：
// 服务端补画可能晚于那一次测量 ✓ ⇒ 偶发读到 0 ✗（本会话反复出现的家族 ✓）。
// 这里量的是**用户最终看到的画面** ✓，而不是某个瞬间 ✓。
let mediumAfter = mediumBefore;
for (let i = 0; i < 40; i++) {
  mediumAfter = (await evaluate(MEDIUM_INK)) || 0;
  // **要等到"比落笔前更多"** ✓ —— 第一版只等 `> 0` ✗，而画布上本来就有前面段落留下的墨 ✓
  // ⇒ 在笔迹出现**之前**就读了数 ✓（实测 74 → 74 的假失败 ✓）。
  if (mediumAfter > mediumBefore) break;
  await new Promise((r) => setTimeout(r, 250));
}
if (!mediumObject) {
  const mediumLog = await evaluate(`document.getElementById("log").innerText.slice(0, 400)`);
  problems.push(
    `介质插件用例失败：status=${JSON.stringify(mediumStatus)}｜对象列表=${JSON.stringify(mediumRawListing).slice(0, 400)}｜日志=${JSON.stringify(mediumLog)}`,
  );
} else {
  if (mediumObject.medium.id !== "example-dab" || mediumObject.medium.version !== 1) {
    problems.push(`介质描述符不正确：${JSON.stringify(mediumObject.medium)}`);
  }
  // **已知问题（如实上报，不假装通过 ✗，也不挡住主线 ✓）**：
  // 介质落笔后画布**变空**（实测 74 → 0 ✓），而对象与描述符都是对的 ✓。
  // 已定位：介质走 heavy 原子（`import_image` ✓），**客户端 WASM 内核表示不了它的像素** ✗，
  // 因此"重新同步内核"（本轮已修 ✓：heavy 原子一律 resync ✓）仍然画不出东西 ✓。
  // 正确修法（下一轮）：heavy 之后不只重载内核 ✓，还要**取服务端该区域的像素贴到内容画布** ✓
  // （设计 14.5「打开即图片」的服务端铺底路径 ✓）。
  // 上一轮这里是"已知问题上报" ⚠（画布空白 ✓）。本轮实现了服务端像素补画 ✓，
  // 因此恢复**严格断言** ✓：介质落笔后画布有墨像素必须增加 ✓。
  // **整笔**的判据 ✓：对象包围盒应明显宽于单个笔尖（48px）✓。
  if (mediumObject && mediumObject.bbox && mediumObject.bbox[2] < 100) {
    problems.push(`介质拖动应产生一整笔（包围盒宽 ${mediumObject.bbox[2]}，期望 >100）`);
  }
  if (mediumAfter <= mediumBefore) {
    const diag = await evaluate(`JSON.stringify({
      resyncs: window.yanshiStats.resyncs, serverBlits: window.yanshiStats.serverBlits,
      kernelHead: window.yanshiStats.kernelHead, serverHead: window.yanshiStats.serverHead,
      board: [board.width, board.height], viewport: state.viewport,
      docSize: state.docSize, lastServerBlitArea: window.yanshiStats.lastServerBlitArea,
      medium: window.yanshiStats.medium,
    })`);
    problems.push(`介质落笔后画布有墨像素应增加（前 ${mediumBefore} → 后 ${mediumAfter}）｜诊断 ${diag}`);
  }
}
// 油画介质（ABI v2）✓：宿主注入笔尖色/目标色/载墨/湿度 ✓，插件做载墨、混色、鬃毛与干湿边缘 ✓。
// 单点落笔已能看出效果 ✓；整笔沿路径铺开是下一步 ✓（记录在 implementation-notes ✓）。
const oilBefore = await evaluate(MEDIUM_INK) || 0;
await evaluate(`(() => {
  const select = document.getElementById("medium");
  if (select) { select.value = "oil"; }
})()`);
await evaluate(`document.querySelector('button[data-tool="medium_dab"]').click()`);
await strokeAt(0.35, 0.3, 502);
let oilObject = null;
for (let i = 0; i < 40 && !oilObject; i++) {
  await new Promise((r) => setTimeout(r, 250));
  const listed = await fetch(
    `${origin}/api/tools/list_objects?doc=${mediumDoc}&token=${mediumToken}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: "{}" },
  ).then((response) => response.json()).catch(() => ({}));
  oilObject = (listed.objects || []).find((o) => o.medium && o.medium.id === "oil") || null;
}
const oilAfter = await evaluate(MEDIUM_INK) || 0;
if (!oilObject) {
  const oilLog = await evaluate(`document.getElementById("log").innerText.slice(0, 300)`);
  problems.push(`油画介质用例失败：日志=${JSON.stringify(oilLog)}`);
} else if (oilObject.medium.version !== 2) {
  problems.push(`油画介质版本应记录为 2，实际 ${JSON.stringify(oilObject.medium)}`);
}
const oilResult = { ok: Boolean(oilObject), medium: oilObject ? oilObject.medium : null,
                    before: oilBefore, after: oilAfter };


// 水彩介质（ABI v2 ✓）：与油画走**同一条宿主链路** ✓，差别全在插件内部 ✓
//（渗开的边界 / 边缘沉积 / 半透明 ✓ —— 三条特征由 `make medium-check` 逐项量过 ✓）。
// 这里只验证**接线** ✓：选择器能选中它 ✓、对象上记录的是水彩的 id 与版本 ✓、画布确实增加 ✓。
// **给水彩这一步一份干净文档**（第 250 轮）：上面 oil 那一步与它**共用** `mediumDoc`，
// 于是水彩落笔之前文档里**已经有**一个带 oil 介质的对象，而查看器有一条**正当**行为
// （`detectHeavyContent`，viewer.rs:1589）：把选择器**同步成"文档里最后一个带介质的对象"**。
// ⇒ 探针每次设水彩都会被改回 oil，**不是产品 bug**，是探针与产品行为相抵。
// ⇒ 换成一份**没有介质对象**的文档，页面自己的规则就与探针一致。
const wcDoc = "uicheck_wc_" + Date.now().toString(36);
const wcToken = await fetch(`${origin}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: wcDoc, width: 320, height: 240 }),
}).then((response) => response.json()).then((value) => value.token);
await send("Page.navigate", { url: `${origin}/?doc=${wcDoc}&token=${wcToken}&debug=1` });
await new Promise((resolve) => setTimeout(resolve, 3500));
const wcBefore = await evaluate(MEDIUM_INK) || 0;
await evaluate(`(() => {
  const select = document.getElementById("medium");
  if (select) {
    // **必须派发 change** ✗（第 243 轮 ✓）：只赋 value 时，页面自己的处理函数**不会跑** ✓
    // ⇒ 于是仍然停在**上一个介质**（oil ✓）⇒ 落墨有 ✓ 但**没有水彩对象** ✗ —— 与实测症状吻合 ✓
    // （before 1960 → after 3102 ✓：墨量涨了 ✓，却找不到 medium.id === "watercolor" ✓）。
    select.value = "watercolor";
    select.dispatchEvent(new Event("change", { bubbles: true }));
  }
  return select ? select.value : null;
})()`);
await evaluate(`document.querySelector('button[data-tool="medium_dab"]').click()`);
// **落笔前一刻再设一次选择器**（第 247 轮）：诊断证明那一笔实际用的是 oil
// （window.yanshiStats.medium 里 id 是 oil、status 是 dabbed），
// 而选择器在点工具之后被重置回了 oil ⇒ 所以要在**点完工具之后、落下之前**设它。
await evaluate(`(() => {
  const select = document.getElementById("medium");
  if (select) { select.value = "watercolor"; select.dispatchEvent(new Event("change", { bubbles: true })); }
  return select ? select.value : null;
})()`);
await dragPath([[0.25, 0.85], [0.45, 0.78], [0.65, 0.86], [0.85, 0.8]], 503);
// **必须在页面里查** ✓（第 244 轮 ✓，与图层面板那段同一先例 ✓）：
// 旧版从 node 用 `mediumDoc` / `mediumToken`（**更早捕获的变量** ✗）查 ⇒
// 若页面已切到别的文档 ⇒ **墨量那一半对 ✓、对象这一半看错了地方** ✗
// （实测：before 1960 → after 3102 ✓ 墨涨了 ✓，却找不到水彩对象 ✗）。
// 经页面自己的入口查（`window.yanshiCallTool` ✓）⇒ **不可能与页面显示的不一致** ✓。
let wcObject = null;
for (let i = 0; i < 40 && !wcObject; i++) {
  await new Promise((r) => setTimeout(r, 250));
  const listed = await evaluate(`(async () => {
    try {
      const result = await window.yanshiCallTool("list_objects", {});
      return JSON.stringify((result && result.objects) || []);
    } catch (error) { return "[]"; }
  })()`);
  let objects = [];
  try { objects = JSON.parse(listed || "[]"); } catch (error) { objects = []; }
  wcObject = objects.find((o) => o.medium && o.medium.id === "watercolor") || null;
}
// 与示例介质同理 ✓：对"有墨"轮询 ✓（补画可能晚于一次测量 ✓ ⇒ 否则偶发读到 0 ✗）。
let wcAfter = wcBefore;
for (let i = 0; i < 40; i++) {
  wcAfter = (await evaluate(MEDIUM_INK)) || 0;
  if (wcAfter > wcBefore) break;
  await new Promise((r) => setTimeout(r, 250));
}
if (!wcObject) {
  // **失败时把已有诊断打出来**（第 246 轮）：判据**早就**收集了它（1608 行），
  // 只是没在失败路径上打印 ⇒ 我为此多花了几轮去猜"文档 / 选择器"。
  // 教训：**已有的证据比新造的可靠**，它当时就在现场。
  const wcDiagnosis = await evaluate(`JSON.stringify(window.yanshiStats.medium || null).slice(0, 300)`);
  problems.push("水彩介质用例失败：未出现带 watercolor 的对象｜诊断 " + String(wcDiagnosis));
} else if (wcObject.medium.version !== 2) {
  problems.push(`水彩介质版本应为 2，实际 ${JSON.stringify(wcObject.medium)}`);
} else if (wcAfter <= wcBefore) {
  // 与介质段同样的诊断 ✓（画布为 0 时最需要知道走的是哪条路径 ✓）。
  const wcDiag = await evaluate(`JSON.stringify({
    resyncs: window.yanshiStats.resyncs, serverBlits: window.yanshiStats.serverBlits,
    lastServerBlitArea: window.yanshiStats.lastServerBlitArea,
    kernelHead: window.yanshiStats.kernelHead, serverHead: window.yanshiStats.serverHead,
    medium: window.yanshiStats.medium,
  })`);
  problems.push(`水彩落笔后画布有墨像素应增加（前 ${wcBefore} → 后 ${wcAfter}）｜诊断 ${wcDiag}`);
}
const wcResult = { ok: Boolean(wcObject), medium: wcObject ? wcObject.medium : null, before: wcBefore, after: wcAfter };

const mediumResult = { ok: Boolean(mediumObject), medium: mediumObject ? mediumObject.medium : null,
                       status: mediumStatus, before: mediumBefore, after: mediumAfter };
// 恢复本段之前的当前图层 ✓。
if (mediumLayerBefore) {
  await evaluate(`(() => {
    const select = document.getElementById("layer");
    if (select && [...select.options].some((o) => o.value === ${JSON.stringify(mediumLayerBefore)})) {
      select.value = ${JSON.stringify(mediumLayerBefore)};
      select.dispatchEvent(new Event("change", { bubbles: true }));
    }
  })()`);
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
    `橡皮没有擦掉内容：着色 ${eraserResult.beforeErase} → ${eraserResult.afterErase}｜日志 ${JSON.stringify((eraserResult.log || "").slice(0, 120))}`
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
if ((retouchResult.log || "").includes("操作失败") || (retouchResult.log || "").includes("错误 ")) {
  problems.push(`修图/液化出现工具错误：${JSON.stringify((retouchResult.log || "").slice(0, 200))}`);
}
if (!(retouchResult.log || "").includes("请先按住 Alt")) {
  problems.push(`仿制图章未设置源点时没有提示：${JSON.stringify((retouchResult.log || "").slice(0, 120))}`);
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
} else if (false) {
  // ~~**不该新增原子**~~ ✗ —— **删掉（第 507 轮 ✓）**：
  // ① 设计 13.2 只说"按原子步进" ✓，**两边都没说"加不加一条原子"** ✗；
  // ② 而**产品实现明说**它是一条 `declare_head` 原子 ✓（`viewer.rs:3622` ✓）⇒ **历史 +1 是它的设计** ✓；
  // ③ ⇒ **我第 476 轮"反过来断言"同样没有依据** ✗ ⇒ ⇒ **它是一条没有设计出处的断言** ✓。
  // ⇒ **只断言设计确实说了的事** ✓：**跳转改变画面** ✓（上面已在断言 ✓）、**撤销能回到跳转前** ✓（下面仍在断言 ✓）。
  // ④ **不是放松** ✓：**"设计没规定的事"不该由判据来规定** ✓ —— 而这一条我已连错两次 ✗（先要求 +1 ✗、再要求 +0 ✗）。
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
// **舞台铺满、画布居中** ✓ —— 用户反馈："画布固定在左上角很难受，尤其缩放时" ✓。
// 旧的断言是"舞台必须贴合画布" ✗（那是为消除右侧死区加的 ✓）——
// 但它的代价正是画布贴左上角 ✓。现在的判据换成两条**更本质**的 ✓：
//   ① 画布在舞台里**居中**（两侧留白对称 ✓，缩放时视觉重心稳定 ✓）；
//   ② 画布外**不响应绘制** ✓（这才是"死区"真正的问题所在 ✓，见下面 stage 点击断言 ✓）。
if (geometry.stage.cssW - geometry.canvas.cssW > 4) {
  const dx = Math.abs(geometry.stage.centerX - geometry.canvas.centerX);
  if (dx > 6) {
    problems.push(`画布在舞台里没有居中：横向偏 ${dx.toFixed(1)}px（舞台宽出 ${geometry.stage.cssW - geometry.canvas.cssW}px）`);
  }
  const dy = Math.abs(geometry.stage.centerY - geometry.canvas.centerY);
  if (dy > 6) {
    problems.push(`画布在舞台里没有居中：纵向偏 ${dy.toFixed(1)}px`);
  }
}
// 画布**外面**点一下：必须**不改像素** ✓（画布周围是工作区 ✓，不是可绘制区域 ✓）。
const outsideClick = JSON.parse(await evaluate(`JSON.stringify((() => {
  const stage = document.querySelector(".stage");
  const rect = board.getBoundingClientRect();
  const stageRect = stage.getBoundingClientRect();
  // 取画布右侧的空白处 ✓（居中后两侧都会留白 ✓）。
  const x = Math.min(stageRect.right - 4, rect.right + 8);
  const y = stageRect.top + stageRect.height / 2;
  if (x <= rect.right) return { skipped: true };
  const before = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
  let beforeN = 0;
  for (let i = 3; i < before.length; i += 4) if (before[i] > 8 && (before[i-3] < 245 || before[i-2] < 245 || before[i-1] < 245)) beforeN++;
  stage.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true,
    pointerId: 970, pointerType: "mouse", isPrimary: true, buttons: 1, clientX: x, clientY: y }));
  stage.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true,
    pointerId: 970, pointerType: "mouse", isPrimary: true, buttons: 0, clientX: x, clientY: y }));
  const after = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
  let afterN = 0;
  for (let i = 3; i < after.length; i += 4) if (after[i] > 8 && (after[i-3] < 245 || after[i-2] < 245 || after[i-1] < 245)) afterN++;
  return { skipped: false, beforeN, afterN, gap: Math.round(rect.right - stageRect.left + (stageRect.right - rect.right)) };
})())`));
if (!outsideClick.skipped && outsideClick.beforeN !== outsideClick.afterN) {
  problems.push(`画布外的空白区不应改变像素（${outsideClick.beforeN} → ${outsideClick.afterN}）`);
}
console.log(`  画布位置：舞台居中偏差 ${Math.abs(geometry.stage.centerX - geometry.canvas.centerX).toFixed(1)}px / ${Math.abs(geometry.stage.centerY - geometry.canvas.centerY).toFixed(1)}px` +
  `｜画布外点击 ${outsideClick.skipped ? "跳过（无留白）" : "不改像素 ✓"}`);

// **平移与缩放** ✓（用户要求："很大时候提供手之类工具移动画布" ✓）。
// 硬判据是"**光标下的文档点保持不动**" ✓ —— 这比"视口数值变了"本质得多 ✓。
const panAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  const docAt = (clientX, clientY) => {
    const rect = board.getBoundingClientRect();
    return {
      x: state.viewport.x + (clientX - rect.left) * board.width / Math.max(1, rect.width),
      y: state.viewport.y + (clientY - rect.top) * board.height / Math.max(1, rect.height),
    };
  };
  // 先放大再平移 —— 用户说的正是画布很大时才需要平移；
  // zoom=1 时整幅文档已适配可见，视口无处可移（第一版测到位移 0 是正确行为，不是 bug）。
  const rect0 = board.getBoundingClientRect();
  // **先放大**（显式入口 ✓ —— 滚轮已改成平移 ✓）。
  {
    const box = document.getElementById("zoomInput");
    box.value = "200";
    box.dispatchEvent(new Event("change", { bubbles: true }));
  }
  const rect = board.getBoundingClientRect();
  const startX = rect.left + rect.width * 0.5;
  const startY = rect.top + rect.height * 0.5;
  const beforeViewport = { x: state.viewport.x, y: state.viewport.y };
  const beforePoint = docAt(startX, startY);
  // 手形工具 + 左键拖动 ✓（空格与中键走同一套代码 ✓）。
  document.querySelector('button[data-tool="pan"]').click();
  board.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, cancelable: true,
    pointerId: 971, pointerType: "mouse", isPrimary: true, buttons: 1, clientX: startX, clientY: startY }));
  const moveX = startX - 60;
  const moveY = startY - 40;
  board.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, cancelable: true,
    pointerId: 971, pointerType: "mouse", isPrimary: true, buttons: 1, clientX: moveX, clientY: moveY }));
  board.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, cancelable: true,
    pointerId: 971, pointerType: "mouse", isPrimary: true, buttons: 0, clientX: moveX, clientY: moveY }));
  const afterViewport = { x: state.viewport.x, y: state.viewport.y };
  const afterPoint = docAt(moveX, moveY);
  // 滚轮缩放：光标处的文档点应保持不动 ✓。
  // **只测一次滚轮** ✓ —— 连测 5 次会把"每次 <1 文档像素的取整误差"累加 ✓
  //（实测 5 次 ≈ 26px ✓，正好是 5×5.3 ✓）而那是**固有约束** ✓ 不是 bug ✓：
  // 视口以**整数文档像素**存储 ✓（内核按整数区域渲染 ✓）。因此按"单次"给容差 ✓。
  const zoomBefore = state.zoom;
  const beforeZoomState = {
    viewport: { x: state.viewport.x, y: state.viewport.y, w: state.viewport.w, h: state.viewport.h },
    scale: state.displayScale, rect: { left: rect.left, width: rect.width },
  };
  // **滚轮必须"只平移、不缩放"** ✓（本轮改的正是这一条 ✓）：这里**用滚轮**，
  // 断言的是"缩放**不变**" ✓ —— 与老版本正好相反 ✓（老版本断言"光标锚点不动"✗，那条契约已删 ✓）。
  const zoomAnchorX = rect.left + rect.width * 0.5;
  const zoomAnchorY = rect.top + rect.height * 0.5;
  board.dispatchEvent(new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: -100, clientX: zoomAnchorX, clientY: zoomAnchorY }));
  const zoomAfter = state.zoom;
  // **锚点漂移**在这条契约下没有意义 ✗（不缩放了 ✓）⇒ 记 0 ✓，真正的断言是"没缩放" ✓。
  const perEventDrift = 0;
  document.querySelector('button[data-tool="brush"]').click();
  const afterZoomRect = board.getBoundingClientRect();
  return {
    beforeZoomState, afterZoomState: {
      viewport: { x: state.viewport.x, y: state.viewport.y, w: state.viewport.w, h: state.viewport.h },
      scale: state.displayScale, rect: { left: afterZoomRect.left, width: afterZoomRect.width },
    },
    moved: Math.abs(afterViewport.x - beforeViewport.x) + Math.abs(afterViewport.y - beforeViewport.y),
    panDrift: Math.hypot(afterPoint.x - beforePoint.x, afterPoint.y - beforePoint.y),
    zoomBefore, zoomAfter,
    zoomDrift: perEventDrift,
    cursor: board.style.cursor,
  };
})())`));
if (panAudit.moved < 1) {
  problems.push(`手形工具拖动没有移动视口（视口位移 ${panAudit.moved}）`);
}
if (panAudit.panDrift > 1.5) {
  problems.push(`平移后光标下的文档点漂移 ${panAudit.panDrift.toFixed(2)}px（应保持不动）`);
}
// **新契约：滚轮只平移、不缩放**（第 251 轮更正）：上面 313 行与下面那段注释都写着这一条，
// 而这里的老断言却要求"滚轮向上必须放大"——**判据自己和自己矛盾**，于是永远报红。
// 真实用户反馈是"想滚动画布的人必然误触缩放"，所以缩放入口只剩显式那几种。
// 反过来断言才正确，而且**照样能红**：哪天滚轮又缩放，这条立刻不成立。
if (panAudit.zoomAfter !== panAudit.zoomBefore) {
  problems.push(`滚轮不该缩放（${panAudit.zoomBefore} → ${panAudit.zoomAfter}）`);
}
// **契约已变** ✓（真实用户实测反馈）：**滚轮只平移、不缩放** ✗ ——
// 触摸板两指滑动与鼠标滚轮是同一个事件 ✓ ⇒ "想滚动画布的人必然误触缩放" ✓，
// 而"滚轮缩放"在定尺画布上不是刚需 ✗。缩放的入口只剩**显式那几种** ✓：
// `适配 / 1:1 / 状态栏百分比输入框 / + - 0 快捷键` ✓。
// ⇒ 这里的断言从"滚轮确实放大"✗ 改成"**滚轮绝不放缩**" ✓（能红 ✓：接回缩放就会红 ✓）。
if (panAudit.zoomAfter !== panAudit.zoomBefore) {
  problems.push(`滚轮改变了缩放（${panAudit.zoomBefore} → ${panAudit.zoomAfter}）—— 它现在只该平移`);
}
if (!(panAudit.moved > 0)) {
  problems.push("手形工具没有移动视口");
}
console.log(`  平移/缩放：视口位移 ${panAudit.moved.toFixed(0)}px，光标下文档点漂移 ${panAudit.panDrift.toFixed(2)}px` +
  `｜滚轮缩放 ${panAudit.zoomBefore.toFixed(2)} → ${panAudit.zoomAfter.toFixed(2)}（必须不变 ✓）`);
console.log("  缩放数值（诊断）：before " + JSON.stringify(panAudit.beforeZoomState) +
  " after " + JSON.stringify(panAudit.afterZoomState));

  // 暗色主题打磨 ✓ —— **量化**验收（不靠"看起来还行" ✗）：
//   ① 正文对比度按 WCAG 公式实算 ≥4.5:1 ✓；② 可点控件高度 ≥24px ✓；
//   ③ 存在 :focus-visible 焦点环规则 ✓；④ 语义令牌可解析 ✓。
//
// 本段整块在**模板字面量内部** ✓ ⇒ 注释里也不能出现反引号 ✗
//（本会话为此连撞三次 ✓：反引号会提前闭合模板 ✓，症状是 missing ) after argument list ✓）。
const themeAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  try {
    const rootStyle = getComputedStyle(document.documentElement);
    const hexToRgb = (value) => {
      const hex = String(value).trim().replace("#", "");
      if (hex.length !== 6) return null;
      return [parseInt(hex.slice(0, 2), 16), parseInt(hex.slice(2, 4), 16), parseInt(hex.slice(4, 6), 16)];
    };
    const luminance = (rgb) => {
      const channel = (v) => {
        const c = v / 255;
        return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
      };
      return 0.2126 * channel(rgb[0]) + 0.7152 * channel(rgb[1]) + 0.0722 * channel(rgb[2]);
    };
    const fg = hexToRgb(rootStyle.getPropertyValue("--text"));
    const bg = hexToRgb(rootStyle.getPropertyValue("--bg"));
    let contrast = null;
    if (fg && bg) {
      const a = luminance(fg);
      const b = luminance(bg);
      contrast = (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
    }
    const controls = [...document.querySelectorAll(
      "header button, #tools button, aside button, .options select, .options input, #quickPanel button, .statusbar button")];
    const heights = controls.map((c) => Math.round(c.getBoundingClientRect().height)).filter((h) => h > 0);
    const minHeight = heights.length ? Math.min(...heights) : 0;
    let focusRule = false;
    for (const sheet of document.styleSheets) {
      for (const rule of sheet.cssRules || []) {
        if (rule.selectorText && rule.selectorText.includes(":focus-visible")) focusRule = true;
      }
    }
    const tokens = ["--bg", "--surface", "--surface-2", "--line", "--text", "--muted", "--accent", "--accent-soft"]
      .map((name) => rootStyle.getPropertyValue(name).trim())
      .filter(Boolean);
    return { contrast, minHeight, focusRule, tokens: tokens.length, controls: controls.length };
  } catch (error) {
    return { error: String(error && error.message ? error.message : error) };
  }
})())`));
if (themeAudit.error) {
  problems.push(`暗色主题检查本身出错（页面侧异常）：${themeAudit.error}`);
} else {
  if (!(themeAudit.contrast >= 4.5)) {
    problems.push(`正文对比度不足 4.5:1（实际 ${themeAudit.contrast === null ? "无法计算" : themeAudit.contrast.toFixed(2)}）`);
  }
  if (themeAudit.minHeight < 24) {
    problems.push(`可点控件偏小：最小高度 ${themeAudit.minHeight}px（须 ≥24px，共 ${themeAudit.controls} 个）`);
  }
  if (!themeAudit.focusRule) {
    problems.push("缺少 :focus-visible 焦点环规则（键盘可达性）");
  }
  if (themeAudit.tokens < 8) {
    problems.push(`语义令牌不完整：只有 ${themeAudit.tokens} 个`);
  }
  console.log(`  暗色主题：正文对比度 ${themeAudit.contrast === null ? "?" : themeAudit.contrast.toFixed(2)}:1（须 ≥4.5）` +
    `｜控件 ${themeAudit.controls} 个，最小高度 ${themeAudit.minHeight}px（须 ≥24）` +
    `｜焦点环 ${themeAudit.focusRule ? "✓" : "✗"}｜令牌 ${themeAudit.tokens}/8`);

// **命中测试优先当前图层** ✓ —— 子 agent 报的 G5：
// 选中某一层用"移动"拖动时，命中测试会命中**所有图层** ✗ ⇒ 拖到了别的层上的全幅背景 ✓
// 并把背景拖出画布 ✓（用户根本没打算动它 ✓）。
// 复现 agent 的场景 ✓：A 层放一个**全幅填充** ✓，B 层放一个小块 ✓，**选中 B** ✓，
// 在两者都覆盖的位置拾取 ✓ ⇒ 必须拾到 B 上的那个对象 ✓。
{
  const docInfo = (await evaluate(`({ docId: window.yanshiStats.docId, token: window.yanshiStats.token })`)) || {};
  const tool = async (name, body) => fetch(`${origin}/api/tools/${name}?doc=${docInfo.docId}&token=${docInfo.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body || {}),
  }).then((r) => r.json()).catch(() => ({}));
  await tool("create_layer", { layer_id: "pick_under", name: "under" });
  await tool("create_layer", { layer_id: "pick_over", name: "over" });
  const filled = await tool("fill", { layer_id: "pick_under", object_id: "pick_bg",
    data: { color: { r: 240, g: 240, b: 240, a: 255 } } });
  const small = await tool("draw_shape", { layer_id: "pick_over", object_id: "pick_small",
    data: { geometry: { kind: "rect", bbox: { x: 40, y: 40, w: 30, h: 30 } },
            color: { r: 20, g: 120, b: 200, a: 255 } } });
  await new Promise((r) => setTimeout(r, 1500));
  const picked = JSON.parse(await evaluate(`(async () => {
    const previous = state.layerId;
    // **选中上面的那个小图层** ✓（正是用户"我正在这一层工作"的状态 ✓）。
    const select = document.getElementById("layer");
    const option = [...select.options].find((o) => o.value === "pick_over");
    if (option) { select.value = "pick_over"; select.dispatchEvent(new Event("change", { bubbles: true })); }
    state.layerId = "pick_over";
    const hit = await pickObjectAt({ x: 55, y: 55 });
    return JSON.stringify({ pickedId: hit ? hit.object_id : null, pickedLayer: hit ? hit.layer_id : null, previous,
                            fillOk: ${filled.ok === true}, smallOk: ${small.ok === true} });
  })()`));
  if (!picked.fillOk || !picked.smallOk) {
    problems.push(`命中测试用例的准备工作失败：${JSON.stringify(picked)}`);
  } else if (picked.pickedLayer !== "pick_over") {
    problems.push(`命中测试没有优先当前图层：拾到 ${picked.pickedId}（图层 ${picked.pickedLayer}），期望 pick_over 上的对象`);
  } else {
    console.log(`  命中测试：拾到 ${picked.pickedId}（图层 ${picked.pickedLayer}，须优先当前图层）`);
  }
}

// **多步拖拽的选区必须提交完整矩形** ✓ —— 子 agent 报的 #4：
// 拖 (64,372)→(432,500) 十步，却提交了 `{w:36.8,h:12.8}` ✗（只取了前两次移动事件 ✓）。
// 同时核对状态栏说真话 ✓（#5：创建选区后仍显示"无选区" ✗，遗留选区会静默裁掉一切 ✓）。
const selectionDrag = JSON.parse(await evaluate(`JSON.stringify((() => {
  const select = document.querySelector('button[data-tool="select_rect"]');
  if (!select) return { error: "找不到选区工具" };
  select.click();
  const rect = board.getBoundingClientRect();
  const toClient = (dx, dy) => ({
    clientX: rect.left + (dx - state.viewport.x) * (rect.width / board.width),
    clientY: rect.top + (dy - state.viewport.y) * (rect.height / board.height),
  });
  const fire = (type, dx, dy) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 991, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...toClient(dx, dy),
  }));
  const from = { x: 64, y: 60 };
  const to = { x: 320, y: 180 };
  fire("pointerdown", from.x, from.y);
  // **十步** ✓（正是子 agent 的复现条件 ✓）。
  for (let i = 1; i <= 10; i++) {
    fire("pointermove", from.x + ((to.x - from.x) * i) / 10, from.y + ((to.y - from.y) * i) / 10);
  }
  fire("pointerup", to.x, to.y);
  return { from, to, expected: { w: to.x - from.x, h: to.y - from.y } };
})())`));
await new Promise((r) => setTimeout(r, 2500));
// **用查看器此刻的文档与令牌** ✓ —— 本会话第三次栽在"查的是旧文档"上 ✗（前面段落会切换文档 ✓）。
const currentDocInfo = (await evaluate(`({ docId: window.yanshiStats.docId, token: window.yanshiStats.token })`)) || {};
const selectionResult2 = await fetch(
  `${origin}/api/tools/list_selections?doc=${currentDocInfo.docId || mediumDoc}&token=${currentDocInfo.token || mediumToken}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: "{}" },
).then((r) => r.json()).catch(() => ({}));
const hintText = await evaluate(`document.getElementById("selectionHint").textContent`);
{
  const selections = selectionResult2.selections || [];
  // 矩形在 `shape.bbox` ✓，不是顶层 `bbox` ✗ —— 我第一版读错字段 ⇒ 明明建了选区却报"没有选区" ✗。
  const last = selections.length ? selections[selections.length - 1] : null;
  const raw = last ? (last.shape && last.shape.bbox) || last.bbox || null : null;
  // 矩形既可能是 `[x,y,w,h]` ✓ 也可能是 `{x,y,w,h}` ✗ —— 我第一版只按下标读 ⇒ 对象时得到 NaN ✓。
  const bbox = Array.isArray(raw) ? raw : (raw && typeof raw === "object" ? [raw.x, raw.y, raw.w, raw.h] : null);
  if (selectionDrag.error) {
    problems.push(`选区拖拽用例未执行：${selectionDrag.error}`);
  } else if (!bbox) {
    problems.push(`选区拖拽后没有选区（期望 ${selectionDrag.expected.w}×${selectionDrag.expected.h}）`);
  } else if (Math.abs(bbox[2] - selectionDrag.expected.w) > 4 || Math.abs(bbox[3] - selectionDrag.expected.h) > 4) {
    problems.push(`多步拖拽提交了错误的矩形：得 ${JSON.stringify(bbox.slice(2))}，期望 ${selectionDrag.expected.w}×${selectionDrag.expected.h}`);
  } else {
    console.log(`  选区拖拽：十步拖出 ${Math.round(bbox[2])}×${Math.round(bbox[3])}（期望 ${selectionDrag.expected.w}×${selectionDrag.expected.h}）｜状态栏「${hintText}」`);
  }
  if (!hintText || hintText.includes("无选区")) {
    problems.push(`创建选区后状态栏仍显示「${hintText}」（应如实报告选区 ✓）`);
  }
  // **本段结束必须清除选区** ✓ —— 否则后面所有绘制都被它裁掉 ✗
  //（实测：紧随其后的文本用例只画出 17 个像素 ✓，正好演示了 #5 的"静默裁掉一切" ✓）。
  await evaluate(`(() => { const b = document.getElementById("clearSelection"); if (b) b.click(); })()`);
  // **轮询**而不是固定等待 ✓：清除是异步的（提交原子 + 重新取选区列表 ✓）。
  let clearedHint = "";
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 250));
    clearedHint = await evaluate(`document.getElementById("selectionHint").textContent`);
    if (clearedHint && clearedHint.includes("无选区")) break;
  }
  if (clearedHint && !clearedHint.includes("无选区")) {
    problems.push(`清除选区后状态栏仍显示「${clearedHint}」`);
  }
}

// **含 CJK 的文本必须响应字号** ✓ —— 这是四位子 agent 报的文本类 bug 中最直观的一条 ✗：
// 字号 42 与 120 得到**完全相同**的 92×14 墨迹 ✓（根因：render 分支先按 5×7 换算缩放，
// 图集路径又按 16 除一次 ⇒ 实际缩放恒为 1 ✓）。修复后按同一份文档量两种字号 ✓。
const textScaleAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  const layer = "uicheck-textsize";
  return { layer };
})())`));
{
  const docId = mediumDoc;
  const createLayer = await fetch(`${origin}/api/tools/create_layer?doc=${docId}&token=${mediumToken}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ layer_id: "uicheck-textsize", name: "text size check" }),
  }).then((r) => r.json()).catch(() => ({}));
  const drawAndMeasure = async (size, objectId) => {
    const drawn = await fetch(`${origin}/api/tools/draw_text?doc=${docId}&token=${mediumToken}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ layer_id: "uicheck-textsize", object_id: objectId,
        data: { text: "中文永", font: "builtin", size, color: { r: 255, g: 0, b: 0, a: 255 },
                position: [8, 8], align: "left" } }),
    }).then((r) => r.json()).catch(() => ({}));
    if (!drawn.ok) return { error: drawn.error_code || "draw failed" };
    const rendered = await fetch(`${origin}/api/tools/render_region?doc=${docId}&token=${mediumToken}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ region: { x: 0, y: 0, w: 260, h: 200 }, raw: true }),
    }).then((r) => r.json()).catch(() => ({}));
    if (!rendered.raw_url) return { error: "no raw_url" };
    const bytes = new Uint8Array(await fetch(`${origin}${rendered.raw_url}`).then((r) => r.arrayBuffer()));
    let ink = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      if (bytes[i] > 128 && bytes[i + 1] < 128 && bytes[i + 2] < 128) ink += 1;
    }
    return { ink };
  };
  if (!createLayer.ok && createLayer.error_code !== "precondition_failed") {
    problems.push(`文本字号用例：新建图层失败 ${JSON.stringify(createLayer).slice(0, 140)}`);
  } else {
    const small = await drawAndMeasure(21, "uicheck_text_small");
    const large = await drawAndMeasure(42, "uicheck_text_large");
    const clear = await fetch(`${origin}/api/tools/delete_object?doc=${docId}&token=${mediumToken}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ object_id: "uicheck_text_small" }),
    }).catch(() => {});
    void clear;
    if (small.error || large.error) {
      problems.push(`文本字号用例未取到像素：${JSON.stringify({ small, large })}`);
    } else if (large.ink <= small.ink * 1.5) {
      problems.push(`含 CJK 的文本没有响应字号（21 → ${small.ink} 像素，42 → ${large.ink} 像素）`);
    } else {
      console.log(`  文本字号：CJK 21 → ${small.ink} 像素｜42 → ${large.ink} 像素（须显著增多）`);
    }
  }
}

// **重新打开一份含介质的文档，画布不能是白板** ✓ —— 这是四位子 agent 独立复现的**阻断性 bug** ✗：
// 文档里只要有 heavy 内容（介质笔画 = `import_image`/`raster_patch` ✓），客户端内核
// `render_region_rgba` 就返回 **len 0** ✓，而查看器原先**直接 return** ✗ ⇒ 首屏全白 ✓，
// 尽管服务端渲染、缩略图、导出**全都正确** ✓（违反设计 14.5「打开即图片」✓）。
// 本段**自包含**：用前面介质段已经画好的文档 ✓，整页重载后直接量画布 ✓。
// 放在检查**最后** ✓ ⇒ 重载不会干扰其它段落 ✓。
// **界面必须如实反映文档状态** ✓（子 agent 报的两条外观项 ✓，都在"重载"这个动作上暴露 ✓）：
//   ① "介质"选择器此前总是回落到 `example` ✗ ⇒ 显示的不是"这份画是用什么画的" ✓
//      而是"上一次点了什么" ✓；
//   ② 没有撤销栈时"撤销"按钮仍**可点** ✗ ⇒ 点了只打印一句提示 ✓，看起来像坏了 ✓。
{
  const docInfo = (await evaluate(`({ docId: window.yanshiStats.docId, token: window.yanshiStats.token })`)) || {};
  const tool = async (name, body) => fetch(`${origin}/api/tools/${name}?doc=${docInfo.docId}&token=${docInfo.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body || {}),
  }).then((r) => r.json()).catch(() => ({}));
  // 用**水彩**画一笔 ✓（与默认的 `example` 不同 ✓，这样才能区分"反映了文档"与"只是默认值" ✓）。
  await evaluate(`(async () => {
    document.getElementById("medium").value = "watercolor";
    document.getElementById("medium").dispatchEvent(new Event("change", { bubbles: true }));
    const points = [];
    for (let i = 0; i <= 6; i++) points.push({ x: 60 + i * 18, y: 60 });
    await mediumStroke("watercolor", points);
    return true;
  })()`);
  await new Promise((r) => setTimeout(r, 2500));
  // 把选择器**手动改回** `example` ✓（模拟"界面状态陈旧" ✓），然后整页重载 ✓。
  await evaluate(`(() => { const s = document.getElementById("medium"); s.value = "example"; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
  // **必须重载"当前这份文档"** ✓ —— 第一版用的是检查开头保存的 `url` ✗，
  // 而检查中途切过文档 ✓ ⇒ 重载回到的是**另一份** ✓，于是永远看不到刚画的水彩 ✓
  //（真机验证时用的是当前文档 ✓ 所以那边是通过的 ✓ —— 环境不一致会让假失败与假通过都出现 ✓）。
  await send("Page.navigate", { url: `${origin}/?doc=${docInfo.docId}` });
  await send("Page.bringToFront", {});
  for (let i = 0; i < 80; i++) {
    const ready = await evaluate(`document.readyState === "complete" && document.querySelectorAll("#tools button").length > 0`);
    if (ready) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  // 等文档的对象读回来（`detectHeavyContent` 是异步的 ✓）。
  let mediumValue = "";
  for (let i = 0; i < 40; i++) {
    mediumValue = await evaluate(`document.getElementById("medium").value`);
    if (mediumValue === "watercolor") break;
    await new Promise((r) => setTimeout(r, 250));
  }
  const undoDisabled = await evaluate(`document.querySelector('button[data-tool="undo"]').disabled`);
  const undoLabel = await evaluate(`document.getElementById("undoDepth").textContent`);
  if (mediumValue !== "watercolor") {
    problems.push(`重载后"介质"选择器没有反映文档：得到「${mediumValue}」，而这份文档最近一笔是 watercolor`);
  } else {
    console.log(`  界面还原：介质选择器 ← ${mediumValue}（须反映文档）｜撤销按钮 disabled=${undoDisabled}｜${undoLabel}`);
  }
  if (undoDisabled !== true) {
    problems.push(`空撤销栈时"撤销"按钮仍可点（disabled=${undoDisabled}｜${undoLabel}）`);
  }
  void tool;
}

const reopenBefore = await evaluate(`(() => {
  const d = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
  let n = 0;
  for (let i = 3; i < d.length; i += 4) if (d[i] > 8 && (d[i-3] < 245 || d[i-2] < 245 || d[i-1] < 245)) n++;
  return n;
})()`) || 0;
await send("Page.navigate", { url });
await send("Page.bringToFront", {});
for (let i = 0; i < 80; i++) {
  const ready = await evaluate(`document.readyState === "complete" && document.querySelectorAll("#tools button").length > 0`);
  if (ready) break;
  await new Promise((r) => setTimeout(r, 250));
}
let reopenAfter = 0;
for (let i = 0; i < 80; i++) {
  reopenAfter = await evaluate(`(() => {
    const d = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
    let n = 0;
    for (let i = 3; i < d.length; i += 4) if (d[i] > 8 && (d[i-3] < 245 || d[i-2] < 245 || d[i-1] < 245)) n++;
    return n;
  })()`) || 0;
  if (reopenAfter > 0) break;
  await new Promise((r) => setTimeout(r, 250));
}
const reopenStats = await evaluate(`JSON.stringify({
  serverBlits: window.yanshiStats.serverBlits, resyncs: window.yanshiStats.resyncs,
  kernelHead: window.yanshiStats.kernelHead, serverHead: window.yanshiStats.serverHead,
})`) || "{}";
if (reopenAfter <= 0) {
  problems.push(`重新打开含介质的文档后画布是白板（重载前 ${reopenBefore} 个有墨像素，重载后 0）｜诊断 ${reopenStats}`);
} else {
  console.log(`  重新打开：画布有墨 ${reopenBefore} → ${reopenAfter}（不得为 0）｜${reopenStats}`);
}

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
console.log(`  本地导入：图层 ${importResult.layersBefore} → ${importResult.layersAfter}｜日志报告成功 ${(importResult.log || "").includes("已导入") ? "✓" : "✗"}（像素由确定性测试覆盖）`);
console.log(`  移动工具：bbox ${JSON.stringify(moveResult.beforeBbox)} → ${JSON.stringify(moveResult.afterBbox)}`);

// **连续两次移动必须累积** ✓ —— 子 agent 报的 #6：`delta` 此前被编成**绝对矩阵** ✗
// ⇒ 第二次移动会**丢掉**第一次 ✓（实测 `dx:30` → 130 ✓，再 `dx:10` → **110** ✗，期望 140 ✓）。
// 单次移动区分不出这个 bug ✓（绝对值与增量在第一步结果相同 ✓），必须连做两次 ✓。
{
  const docNow = (await evaluate(`({ docId: window.yanshiStats.docId, token: window.yanshiStats.token })`)) || {};
  const boxOf = async (objectId) => {
    const listed = await fetch(`${origin}/api/tools/list_objects?doc=${docNow.docId}&token=${docNow.token}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: "{}",
    }).then((r) => r.json()).catch(() => ({}));
    const found = (listed.objects || []).find((o) => o.object_id === objectId);
    return found && found.bbox ? found.bbox : null;
  };
  const target = (await fetch(`${origin}/api/tools/list_objects?doc=${docNow.docId}&token=${docNow.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: "{}",
  }).then((r) => r.json()).then((d) => (d.objects || []).find((o) => o.type === "shape" || o.type === "stroke")).catch(() => null));
  if (!target) {
    problems.push("连续移动用例：当前文档里找不到可移动对象");
  } else {
    const start = await boxOf(target.object_id);
    for (let i = 0; i < 2; i++) {
      await fetch(`${origin}/api/tools/move_object?doc=${docNow.docId}&token=${docNow.token}`, {
        method: "POST", headers: { "content-type": "application/json" },
        body: JSON.stringify({ object_id: target.object_id, delta: { dx: 25, dy: 0 } }),
      }).catch(() => {});
    }
    await new Promise((r) => setTimeout(r, 1200));
    const moved = await boxOf(target.object_id);
    if (!start || !moved) {
      problems.push(`连续移动用例未能读到包围盒：${JSON.stringify({ start, moved })}`);
    } else if (Math.abs((moved[0] - start[0]) - 50) > 2) {
      problems.push(`连续两次 delta{dx:25} 只累积了 ${(moved[0] - start[0]).toFixed(0)}px（应为 50，说明 delta 被当成绝对值）`);
    } else {
      console.log(`  连续移动：两次 delta{dx:25} 累积 ${(moved[0] - start[0]).toFixed(0)}px（须 50）`);
    }
  }
}
console.log(`  水彩介质：${wcResult && wcResult.ok ? "对象介质 " + JSON.stringify(wcResult.medium) + "｜画布 " + wcResult.before + " → " + wcResult.after : "失败 " + JSON.stringify(wcResult)}`);

// **设计 11.1 的其余介质** ✓：马克笔与铅笔走**同一条**用户路径 ✓
//（选中介质 ⇒ 用"介质"工具落笔 ✓ ⇒ 服务端记录 `medium {id, version}` ✓ ⇒ 画布有墨 ✓）。
// 断言刻意与已有介质用例一致 ✓：**只有"能画出来且描述符正确"才算通过** ✓。
for (const [key, id, label] of [["marker", "marker", "马克笔"], ["pencil", "pencil", "铅笔"], ["pixel", "pixel", "像素"]]) {
  const docId = `${mediumDoc}-${key}`;
  const created = await fetch(`${origin}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width: 420, height: 320 }),
  }).then((r) => r.json()).catch(() => ({}));
  if (!created.token) {
    problems.push(`${label}用例：新建文档失败`);
    continue;
  }
  await send("Page.navigate", { url: `${origin}/?doc=${docId}&token=${created.token}` });
  await send("Page.bringToFront", {});
  for (let i = 0; i < 80; i++) {
    const ready = await evaluate(`document.readyState === "complete" && document.querySelectorAll("#tools button").length > 0 && window.yanshiStats.wasm`);
    if (ready) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  const before = (await evaluate(MEDIUM_INK)) || 0;
  const drawn = JSON.parse(await evaluate(`(async () => {
    const select = document.getElementById("medium");
    if (![...select.options].some((o) => o.value === ${JSON.stringify(key)})) {
      return JSON.stringify({ error: "选择器里没有这个介质" });
    }
    select.value = ${JSON.stringify(key)};
    select.dispatchEvent(new Event("change", { bubbles: true }));
    if (typeof syncStrengthLabel === "function") syncStrengthLabel();
    document.querySelector('button[data-tool="medium_dab"]').click();
    document.getElementById("size").value = 26;
    const points = [];
    for (let i = 0; i <= 10; i++) points.push({ x: 40 + i * 24, y: 120 + Math.sin(i / 2) * 26 });
    try {
      await mediumStroke(${JSON.stringify(key)}, points);
    } catch (error) {
      return JSON.stringify({ error: String(error && error.message || error) });
    }
    return JSON.stringify({ ok: true, label: document.getElementById("strengthLabel").textContent });
  })()`));
  if (drawn.error) {
    problems.push(`${label}用例落笔失败：${drawn.error}`);
    continue;
  }
  let after = before;
  for (let i = 0; i < 40; i++) {
    after = (await evaluate(MEDIUM_INK)) || 0;
    if (after > before) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  const listed = await fetch(`${origin}/api/tools/list_objects?doc=${docId}&token=${created.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: "{}",
  }).then((r) => r.json()).catch(() => ({}));
  const recorded = (listed.objects || []).filter((o) => o.medium && o.medium.id === id);
  const version = recorded.length ? recorded[recorded.length - 1].medium.version : null;
  if (recorded.length === 0) {
    problems.push(`${label}：对象里没有记录 medium {id: ${id}}`);
  } else if (after <= before) {
    problems.push(`${label}：落笔后画布没有新增墨迹（${before} → ${after}）`);
  } else {
    console.log(`  ${label}介质：${recorded.length} 个对象记录 ${id} v${version}｜画布 ${before} → ${after}｜湿度标签「${drawn.label}」`);
  }
}

console.log(`  油画介质：${oilResult && oilResult.ok ? "对象介质 " + JSON.stringify(oilResult.medium) + "｜画布 " + oilResult.before + " → " + oilResult.after : "失败 " + JSON.stringify(oilResult)}`);
console.log(`  介质插件：${mediumResult && mediumResult.ok ? "对象介质 " + JSON.stringify(mediumResult.medium) + "｜画布 " + mediumResult.before + " → " + mediumResult.after : "失败 " + JSON.stringify(mediumResult)}`);

// **介质工效** ✓ —— 子 agent 报的 F1/F2：
//   F1：此前**每一笔介质都新建一个图层** ✗（37 笔 ⇒ 37 层 ✓，还顺手改走 `state.layerId` ✓，
//       于是"在一层里画完"根本做不到 ✓）；
//   F2：笔尖尺寸写死 `min(48, maxDab)` ✗ ⇒ **"粗细"滑杆对介质完全无效** ✓
//       （实测 #size 12/24/32/40/48 画出的色带宽度都是 54~56px ✓）。
{
  const docInfo = (await evaluate(`({ docId: window.yanshiStats.docId, token: window.yanshiStats.token })`)) || {};
  const tool = async (name, body) => fetch(`${origin}/api/tools/${name}?doc=${docInfo.docId}&token=${docInfo.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body || {}),
  }).then((r) => r.json()).catch(() => ({}));
  const layersBefore = ((await tool("list_layers")).layers || []).length;
  const objectsBefore = ((await tool("list_objects")).objects || []).filter((o) => o.type === "raster_patch").length;
  // 用查看器自己的绘制路径 ✓（这里直接驱动页面里的 `mediumStroke` ✓，与用户落笔同一条路径 ✓）。
  const strokeAt = async (tipSize, y) => {
    return await evaluate(`(async () => {
      document.getElementById("size").value = ${tipSize};
      const points = [];
      for (let i = 0; i <= 8; i++) points.push({ x: 40 + i * 20, y: ${y} });
      await mediumStroke("example", points);
      return true;
    })()`);
  };
  await strokeAt(12, 260);
  await new Promise((r) => setTimeout(r, 2500));
  await strokeAt(48, 380);
  await new Promise((r) => setTimeout(r, 2500));
  const layersAfter = ((await tool("list_layers")).layers || []).length;
  const patches = ((await tool("list_objects")).objects || []).filter((o) => o.type === "raster_patch");
  const fresh = patches.slice(-2);
  if (layersAfter > layersBefore + 1) {
    problems.push(`两笔介质新增了 ${layersAfter - layersBefore} 个图层（应当落在选中的图层里，最多兜底新建 1 个）`);
  }
  if (fresh.length < 2) {
    problems.push(`介质工效用例：两笔之后只找到 ${fresh.length} 个新斑点`);
  } else {
    const thin = Number(fresh[0].bbox && fresh[0].bbox[3]) || 0;
    const thick = Number(fresh[1].bbox && fresh[1].bbox[3]) || 0;
    if (!(thick > thin * 1.5)) {
      problems.push(`"粗细"滑杆对介质无效：笔尖 12 → 高 ${thin}px，笔尖 48 → 高 ${thick}px`);
    } else {
      console.log(`  介质工效：两笔新增图层 ${layersAfter - layersBefore} 个（须 ≤1）｜笔尖 12 → ${thin}px，48 → ${thick}px（须显著变宽）`);
    }
  }
  void objectsBefore;
  // **"强度 / 湿度"标签必须随介质说真话** ✓ —— 子 agent 报的 F3：
  // 该滑杆在插件介质下喂的是 `wetness` ✓ ⇒ 越大越湿、颜色越淡 ✓
  //（实测 alpha：85 → 0.238 ✓、55 → 0.482 ✓），继续叫"强度"会让人以为越大越浓 ✗。
  const labelFor = async (mediumValue) => {
    await evaluate(`(() => { const s = document.getElementById("medium"); s.value = ${JSON.stringify(mediumValue)}; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
    await new Promise((r) => setTimeout(r, 200));
    return await evaluate(`document.getElementById("strengthLabel").textContent`);
  };
  const pluginLabel = await labelFor("watercolor");
  const brushLabel = await labelFor("example");
  await labelFor("example");
  if (pluginLabel !== "湿度") {
    problems.push(`插件介质下"强度"滑杆的标签应为「湿度」（实际「${pluginLabel}」）`);
  }
  if (brushLabel !== "强度") {
    problems.push(`内置介质下该滑杆的标签应为「强度」（实际「${brushLabel}」）`);
  }
  // **介质落笔后应当立刻按这一笔的区域补画** ✓ —— 子 agent 报的 F4（约 1 秒白闪 ✗）。
  // 判据 ✓：早期补画确实发生 ✓，且它传的字节**远小于全视口** ✓（即真的是增量 ✓）。
  const early = JSON.parse(await evaluate(`JSON.stringify({
    count: window.yanshiStats.mediumEarlyBlits || 0,
    area: window.yanshiStats.lastEarlyBlitArea || 0,
    lastFull: window.yanshiStats.lastServerBlitArea || 0,
  })`));
  // **干介质（铅笔）的标签必须是"强度"** ✓ —— 它忽略湿度 ✓、按压力上墨 ✓。
  // 我第一版把标签写成"插件即湿度" ✗ ⇒ 铅笔（干介质）也被标成"湿度" ✓，同样是误导 ✓。
  const pencilLabel = await (async () => {
    const setMedium = async (value) => {
      await evaluate(`(() => { const s = document.getElementById("medium"); s.value = ${JSON.stringify(value)}; s.dispatchEvent(new Event("change", { bubbles: true })); })()`);
      await new Promise((r) => setTimeout(r, 200));
      return await evaluate(`document.getElementById("strengthLabel").textContent`);
    };
    const label = await setMedium("pencil");
    await setMedium("example");
    return label;
  })();
  if (pencilLabel !== "强度") {
    problems.push(`干介质（铅笔）下该滑杆的标签应为「强度」（实际「${pencilLabel}」）`);
  } else {
    console.log(`  干介质标签：铅笔「${pencilLabel}」（须为强度，因为它按压力上墨）`);
  }
  if (!early || early.count < 1) {
    problems.push(`介质落笔后没有"立刻局部补画"（计数 ${early ? early.count : "?"}，应 ≥1）`);
  } else if (!(early.area > 0 && early.area < early.lastFull)) {
    problems.push(`早期补画不是增量的（早期 ${early.area}px²，全量 ${early.lastFull}px²）`);
  } else {
    console.log(`  介质早期补画：${early.count} 次｜本次 ${early.area}px² vs 全量 ${early.lastFull}px²（须更小）`);
  }
  // **重同步必须走增量** ✓ —— 子 agent 报的 F5：每笔介质都整条重放日志 ✗
  // ⇒ 提交耗时随文档增长（~1.1s → ~4–5s ✓）。
  // 判据 ✓：续传确实发生 ✓，且**一次续传应用的原子数远少于整份文档** ✓（即真的没重放 ✓）。
  const resyncAudit = JSON.parse(await evaluate(`JSON.stringify({
    incremental: window.yanshiStats.incrementalResyncs || 0,
    fallbacks: window.yanshiStats.resyncFallbacks || 0,
    lastAtoms: window.yanshiStats.lastResyncAtoms || 0,
    resyncs: window.yanshiStats.resyncs || 0,
    head: window.yanshiStats.serverHead || 0,
  })`));
  if (!resyncAudit || resyncAudit.incremental < 1) {
    problems.push(`介质落笔后没有发生过增量续传（增量 ${resyncAudit ? resyncAudit.incremental : "?"} 次，应 ≥1）`);
  } else if (!(resyncAudit.lastAtoms < Math.max(1, resyncAudit.head))) {
    // **应用 0 个原子是合法的** ✓（内核已经同步 ✓，这恰恰说明没有重放 ✓）——
    // 判据只要求"**少于整份文档**" ✓，不要求"大于 0" ✗（我第一版写错 ✓）。
    problems.push(`续传并非增量：上次应用 ${resyncAudit.lastAtoms} 个原子，而文档共有 ${resyncAudit.head} 个`);
  } else {
    console.log(`  增量续传：${resyncAudit.incremental} 次（其中退回全量 ${resyncAudit.fallbacks} 次）｜上次应用 ${resyncAudit.lastAtoms} 个原子 vs 文档 ${resyncAudit.head} 个`);
  }
  if (pluginLabel === "湿度" && brushLabel === "强度") {
    console.log(`  强度/湿度标签：插件介质「${pluginLabel}」｜内置介质「${brushLabel}」（须随介质改名）`);
  }
}
// 光标处快捷面板 ✓（借鉴 Krita 的 Pop-up Palette ✓）。
// 断言四件事 ✓：右键弹出且**靠近光标** ✓、内容来自**同一份定义** ✓、选色会驱动既有控件 ✓、Esc 能关 ✓。
const quickAudit = JSON.parse(await evaluate(`JSON.stringify((() => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const x = rect.left + rect.width * 0.5;
  const y = rect.top + rect.height * 0.5;
  const open = () => board.dispatchEvent(new MouseEvent("contextmenu", {
    bubbles: true, cancelable: true, clientX: x, clientY: y,
  }));
  open();
  const panel = document.getElementById("quickPanel");
  const box = panel.getBoundingClientRect();
  // **断言"产品的契约"，不是"某个角点恰好接近"** ✓（第 237 轮更正 ✓）：
  // 产品把面板放在光标处 ✓、并**夹进视口** ✓（viewer.rs:7901–7904 ✓："夹在视口内" 是它自己的注释 ✓）。
  // ⇒ 视口**比光标下方的空间还矮**时 ✓，面板**必然**离光标很远 ✓ —— 实测：光标 y=350 ✓、视口 437 ✓、
  // 面板高 211 ✓ ⇒ top = min(350, 437-211-6) = **220** ✓ ⇒ 距离 **130** ✗ ⇒ 旧断言**不可能成立** ✗。
  // ⇒ 正确的断言：**面板位置 == "光标夹进视口"** ✓（这既覆盖"放在光标处" ✓、也覆盖"不越界" ✓），
  //    而且它**能红** ✓：产品若不再夹取或不再跟随光标 ✓ ⇒ 这个等式立刻不成立 ✓。
  const expectedLeft = Math.max(6, Math.min(x, window.innerWidth - box.width - 6));
  const expectedTop = Math.max(6, Math.min(y, window.innerHeight - box.height - 6));
  const near = Math.abs(box.left - expectedLeft) <= 2 && Math.abs(box.top - expectedTop) <= 2;
  const withinViewport = box.left >= 0 && box.top >= 0 &&
    box.right <= window.innerWidth + 1 && box.bottom <= window.innerHeight + 1;
  const counts = {
    mediums: panel.querySelectorAll("[data-qp-medium]").length,
    colors: panel.querySelectorAll("[data-qp-color]").length,
    sizes: panel.querySelectorAll("[data-qp-size]").length,
  };
  const swatch = panel.querySelector('[data-qp-color="#2f9e44"]');
  if (swatch) swatch.click();
  const colorAfter = document.getElementById("color").value;
  const closedAfterPick = panel.hidden;
  open();
  const reopened = !panel.hidden;
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  const closedByEscape = panel.hidden;
  return { near, withinViewport, counts, colorAfter, closedAfterPick, reopened, closedByEscape };
})())`));
if (!quickAudit.near) {
  problems.push("快捷面板没有出现在光标附近");
}
if (!quickAudit.withinViewport) {
  problems.push("快捷面板超出了视口（贴边右键时应被夹住）");
}
if (quickAudit.counts.mediums < 3 || quickAudit.counts.colors < 6 || quickAudit.counts.sizes < 3) {
  problems.push(`快捷面板内容不完整：${JSON.stringify(quickAudit.counts)}`);
}
if (quickAudit.colorAfter !== "#2f9e44") {
  problems.push(`快捷面板选色没有驱动 #color（实际 ${quickAudit.colorAfter}）`);
}
if (!quickAudit.closedAfterPick || !quickAudit.reopened || !quickAudit.closedByEscape) {
  problems.push(`快捷面板开合不对：${JSON.stringify(quickAudit)}`);
}
console.log(`  快捷面板：介质 ${quickAudit.counts.mediums}｜颜色 ${quickAudit.counts.colors}｜笔尖 ${quickAudit.counts.sizes}` +
  `｜贴光标 ${quickAudit.near ? "✓" : "✗"}｜视口内 ${quickAudit.withinViewport ? "✓" : "✗"}｜Esc 关 ${quickAudit.closedByEscape ? "✓" : "✗"}`);

console.log(`  选区/文本：${selectionResult && selectionResult.ok
  ? `选区内笔画色 ${selectionResult.inside}｜选区外 ${selectionResult.outside}（须 0）｜清除后选区外 ${selectionResult.clearedOutside}（须 >0）｜文本改变 ${selectionResult.textChanged}`
  : `失败于阶段「${selectionResult && selectionResult.stage}」：${JSON.stringify(selectionResult)}`}`);
console.log(`  新建/另存为：预填 ${JSON.stringify(namingResult.prefilled)}｜identity ${JSON.stringify(namingResult.identity)}｜副本内容 ${namingResult.before} → ${namingResult.after}`);
console.log(`  蒙版编辑：填充后着色 ${maskResult.filled} → 加矩形蒙版后 ${maskResult.masked}`);
// **工具状态必须打出来** ✓（第 468 轮 ✓）：第 467 轮只是把它 return 了 ✗ ⇒ **返回值不进日志** ✓
// ⇒ 不打印就等于没取 ✓ —— 与「算了不断言」「探针取晚」同族 ✓。
console.log(`  蒙版：点击按钮后的工具 ⇒ ${JSON.stringify(maskResult.toolAfterClick)}（应为 mask_rect ✓）`);
// **这里不能再切短** ✓（第 584 轮 ✓）：变量已取 `slice(-160)` ✓，而这一行又 `slice(-120)` ✗
// ⇒ **我追加在尾部的 `move1后=/move2后=/抬手前=` 被它截掉** ✓（日志里只剩到 `dragging=301` ✓）。
// ⇒ 放宽到 `-260` ✓ —— **追加的内容在尾部 ✓，尾巴要留够** ✓。
console.log(`  蒙版拖动期间的日志尾部 ⇒ ${JSON.stringify((maskResult.logDuringDrag || "").slice(-260))}`);
console.log(`  布局：scrollWidth ${overflow.scrollWidth} / clientWidth ${overflow.clientWidth}｜body ${overflow.bodyWidth}｜main ${overflow.mainWidth}｜侧栏 ${overflow.asideWidth}`);
console.log(`  最靠右的元素：${overflow.widest.join(", ")}`);
console.log(`  dialog.open=${overflow.dialogOpen}｜自身超宽的：${overflow.scrollWide.join(", ")}`);
console.log(`  工具栏：${toolbar.total} 个按钮，视口外 ${toolbar.outside.length} 个`);
console.log(`  橡皮：着色 ${eraserResult.beforeErase} → ${eraserResult.afterErase}`);
console.log(`  打开已有作品（重载后）着色：${paintedAfterReload}`);
console.log(`  回到此处可撤销：跳转前后指纹 ${jumpUndoResult?.ok ? `${fingerprintBeforeJump.sum} → ${jumpUndoResult.jumped.sum} → 撤销后 ${jumpUndoResult.undone.sum}` : "未执行"}`);
console.log(`  吸管/填充：吸管取到 ${pickResult.picked}（期望 #c81e3c）｜填充指纹 ${beforeFill.sum} → ${afterFill.sum} → 刷新 ${afterFillRefresh.sum}（撤销填充由确定性测试覆盖）`);
console.log(`  修图/液化：液化推后指纹 ${retouchBefore.sum} → ${retouchAfter.sum}（不透明 ${retouchAfter.opaque}/${retouchAfter.total}）｜仿制无源点有提示 ${(retouchResult.log || "").includes("请先按住 Alt") ? "✓" : "✗"}`);
console.log(`  历史浏览：${historyBefore.length} 条 → 提交后 ${historyAfter.length} 条（含 draw_stroke ✓、筛选 ${filteredRows.length} 行 ✓）→ 回到此处后 ${historyFinal.length} 条`);
console.log(`  导出 PNG：${exportPng ? `${exportPng.width}×${exportPng.height}，${(exportPng.bytes/1024).toFixed(0)} KB` : "无"}`);
console.log(`  一笔的 draw_stroke 日志条数：${atomLines}｜右侧面板右边界 ${layout.asideRight} / 视口 ${layout.viewport}`);
console.log(`  着色像素时间线（每 250ms）：${timeline.join(" → ")}`);
console.log(`  几何：canvas ${geometry.canvas.w}×${geometry.canvas.h}（CSS ${geometry.canvas.cssW}×${geometry.canvas.cssH}）｜preview ${JSON.stringify(geometry.preview)}`);
console.log(`  缩略图：${thumbBefore === thumbAfter ? "未变化" : "已自动刷新"}`);
console.log(`  'tiles 个失效' 噪声行：${tileNoise}`);
// **图层面板** ✓（用户点名的图层功能：列表 / 上下移动 / 锁定 / 显示隐藏 / 复制 ✓）。
//
// 断言读**面板 DOM + 服务端 `list_layers`** 两边 ✓，并交叉核对 ✓ ——
// 只读 DOM 只能证明"画出来了" ✗，只读服务端只能证明"数据对" ✗；
// **两边一致**才说明面板真的反映了真相 ✓（本项目吃过"面板与实际漂移"的亏 ✓）。
const layerPanel = await evaluate(`(async () => {
  // **判墨口径** ✓：与脚本别处一致（不看纯白背景 ✓）—— 直接在页面里量 ✓，
  // 因为"隐藏→量→再显示→量"之间不能回到 Node（那会把状态拆成多次 evaluate ✓，
  // 中间还可能被重画打断 ✓）。
  const ink = () => {
    const board = document.getElementById("board");
    const data = board.getContext("2d").getImageData(0, 0, board.width, board.height).data;
    let count = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 8 && (data[i] < 200 || data[i + 1] < 200 || data[i + 2] < 200)) count++;
    }
    return count;
  };
  const rows = () => [...document.querySelectorAll("#layerList .layer-row")];
  const ids = () => rows().map((row) => row.dataset.layerId);
  const out = {};
  out.rowCount = rows().length;
  // **记住"还没新增/复制之前"的选中层**（第 307 轮更正）：新增与复制都会把选中项移到新建的那一层，
  // 而画面上的墨在**原来那层**上；隐藏若不作用在原来那层，隐藏的就是空层或副本，画面当然不变。
  const originalLayerId = document.getElementById("layer")?.value || rows()[0]?.dataset.layerId || null;
  out.ids = ids();
  out.selected = document.querySelector("#layerList .layer-row.selected")?.dataset.layerId || null;
  out.current = document.getElementById("layer")?.value || null;
  // 服务端真相 ✓（自下而上 ✓）。
  // **必须走查看器自己的调用入口** ✓ —— 我第一版直接 「fetch('/api/tools/list_layers')」 ✗，
  // 少了 ?doc=&token= ✓ ⇒ 服务端返回错误 ✓、layers 为空 ✓ ⇒ 断言读到 [] ✗
  //（"服务端自下而上 []" ✓ 就是这条假失败 ✓）。查看器已经把带凭据的入口暴露成
  // window.yanshiCallTool ✓ ⇒ 用它 ✓，读到的才是真正的服务端状态 ✓。
  const listed = await window.yanshiCallTool("list_layers", {});
  out.serverOrder = (listed.layers || []).map((layer) => layer.layer_id);
  // **面板应自上而下显示** ✓ ⇒ 第一行应当是服务端的**最后一个** ✓。
  out.firstRowIsTop = out.ids[0] === out.serverOrder[out.serverOrder.length - 1];
  // ① **新建** ⇒ 多一行 ✓。
  document.getElementById("layerAdd").click();
  await new Promise((r) => setTimeout(r, 900));
  out.afterAdd = rows().length;
  try {
    const listed = await window.yanshiCallTool("list_objects", {});
    const objects = (listed && listed.objects) || [];
    const counts = {};
    for (const o of objects) { if (o.layer_id) counts[o.layer_id] = (counts[o.layer_id] || 0) + 1; }
    out.objectsPerLayer = counts;
    let withLayer = null;
    for (const id of Object.keys(counts)) {
      if (!withLayer || counts[id] > counts[withLayer.layer_id]) withLayer = { layer_id: id };
    }
    if (!withLayer) withLayer = objects.find((o) => o.layer_id) || null;
    const select = document.getElementById("layer");
    out.switchedFrom = select ? select.value : null;
    if (withLayer && select && select.value !== withLayer.layer_id) {
      select.value = withLayer.layer_id;
      if (typeof select.onchange === "function") select.onchange();
      await new Promise((r) => setTimeout(r, 1000));
    }
    out.switchedTo = withLayer ? withLayer.layer_id : "none";
    out.rowsAfterSwitch = rows().map((row) => row.dataset.layerId);
  } catch (error) {
    out.switchFailed = String((error && error.message) || error);
  }
  // ② **复制** ⇒ 多一行 ✓，且名字带"副本" ✓。
  document.getElementById("layerDuplicate").click();
  await new Promise((r) => setTimeout(r, 1200));
  out.afterDuplicate = rows().length;
  out.copyName = rows().map((row) => row.querySelector(".layer-name")?.textContent || "")
    .find((text) => text.includes("副本")) || null;
  // ③ **上移** ⇒ 顺序变化 ✓（比较行序快照 ✓）。
  const beforeMove = ids().join(",");
  const selectedBefore = document.getElementById("layer")?.value;
  document.getElementById("layerUp").click();
  await new Promise((r) => setTimeout(r, 900));
  out.moveChanged = ids().join(",") !== beforeMove;
  out.selectedKept = document.getElementById("layer")?.value === selectedBefore;
  document.getElementById("layerDown").click();
  await new Promise((r) => setTimeout(r, 900));
  // ④ **显示/隐藏** ⇒ 画布墨量变化 ✓（把有内容的图层藏起来 ✓ ⇒ 墨量应变为 0 ✓）。
  const paintedBefore = ink();
  // **每次都要重新查询行** ✓ —— 我第一版把行元素**存下来**、点第二次时再用 ✗，
  // 而 refreshLayers() 每次都会**重建整个列表** ✓ ⇒ 存下来的节点已经**脱离文档** ✓
  // ⇒ 第二次点击打在空气上 ✗（症状：隐藏成功、显示回来却没反应 ✓，看起来像产品 bug ✗）。
  // 这与本项目早先遇到的"过期选择/过期元素"是同一类 ✓。
  const rowById = (id) => rows().find((row) => row.dataset.layerId === id);
  // **取"当下活动的图层"**（第 248 轮）：这一段原先四处硬编码了一个并不存在的 id
  // （layer_paint），于是每一处都静默失败：隐藏/显示量到的墨量不变、
  // 锁定读到的图标是 null。改为取**面板此刻选中的那一层**，四处都用它，
  // 而"面板 DOM"与"服务端 list_layers"仍然核对**同一个 id**，交叉核对没有削弱。
  // **把选中项切回"原来那层"**：复制那一步的处理器会把选中项设成副本，
  // 于是 targetLayerId 会是副本，隐藏与锁定就都打在副本上。
  try {
    const select = document.getElementById("layer");
    out.selectedBeforeHide = select ? select.value : null;
    if (originalLayerId && select && select.value !== originalLayerId) {
      select.value = originalLayerId;
      if (typeof select.onchange === "function") select.onchange();
      await new Promise((r) => setTimeout(r, 1200));
    }
    out.selectedForHide = select ? select.value : null;
  } catch (error) {
    out.selectForHideFailed = String((error && error.message) || error);
  }
  const targetLayerId = document.getElementById("layer")?.value || rows()[0]?.dataset.layerId || null;

  const clickFlag = async (id, action) => {
    const row = rowById(id);
    if (!row) return false;
    row.querySelector('[data-action="' + action + '"]').click();
    return true;
  };
  out.hidContentRow = await clickFlag(targetLayerId, "visible");
  // **轮询而不是固定 sleep** ✓：重画要经过"工具返回 → resync → rAF 补画" ✓，
  // 固定 1200ms 在冷启动时不够、在热路径上又白等 ✓。
  for (let i = 0; i < 40 && ink() !== 0; i++) await new Promise((r) => setTimeout(r, 150));
  out.paintedAfterHide = ink();
  out.hideDiag = { target: targetLayerId, clicked: out.hidContentRow === true,
    serverVisible: (() => { const l = (listed.layers || []).find((x) => x.layer_id === targetLayerId); return l ? l.visible : "no-row"; })(),
    rows: rows().map((r) => r.dataset.layerId) };
  await clickFlag(targetLayerId, "visible");
  for (let i = 0; i < 40 && ink() === 0; i++) await new Promise((r) => setTimeout(r, 150));
  out.paintedAfterShow = ink();
  out.paintedBefore = paintedBefore;
  // ⑤ **锁定** ⇒ 面板状态变 ✓（并按设计**阻止改内容** ✓）。
  const lockRow = rows().find((row) => row.dataset.layerId === targetLayerId);
  if (lockRow) {
    lockRow.querySelector('[data-action="locked"]').click();
    await new Promise((r) => setTimeout(r, 900));
  }
  const listed2 = await window.yanshiCallTool("list_layers", {});
  out.lockedOnServer = (listed2.layers || []).some((layer) => layer.layer_id === targetLayerId && layer.locked);
  out.lockIcon = lockRow ? lockRow.querySelector('[data-action="locked"]')?.textContent : null;
  return out;
})()`);

const layerProblems = [];
if (!layerPanel) layerProblems.push("图层面板未取到状态");
// **一行是正常的起点** ✓（检查用的文档只有一个图层 ✓）—— 我第一版要求"至少两行" ✗，
// 那是把"＋ 之后"的期望写到了"之前" ✓。
if ((layerPanel?.rowCount || 0) < 1) {
  layerProblems.push(`图层面板应至少一行（实得 ${layerPanel?.rowCount}）`);
}
if (!layerPanel?.firstRowIsTop) {
  layerProblems.push(
    `面板应自上而下显示最上层（第一行 ${layerPanel?.ids?.[0]}，服务端自下而上 ${JSON.stringify(layerPanel?.serverOrder)}）`,
  );
}
if (layerPanel?.selected !== layerPanel?.current) {
  layerProblems.push(`面板选中项应与 #layer 一致（面板 ${layerPanel?.selected} vs 选择器 ${layerPanel?.current}）`);
}
if ((layerPanel?.afterAdd || 0) !== (layerPanel?.rowCount || 0) + 1) {
  layerProblems.push(`「＋」应新增一行（${layerPanel?.rowCount} → ${layerPanel?.afterAdd}）`);
}
if ((layerPanel?.afterDuplicate || 0) !== (layerPanel?.afterAdd || 0) + 1) {
  layerProblems.push(`「⧉」应新增一行（${layerPanel?.afterAdd} → ${layerPanel?.afterDuplicate}）`);
}
if (!String(layerPanel?.copyName || "").includes("副本")) {
  layerProblems.push(`复制出来的图层名应含「副本」（实得 ${layerPanel?.copyName}）`);
}
if (!layerPanel?.moveChanged) layerProblems.push("「↑」应改变图层顺序");
if (!layerPanel?.selectedKept) layerProblems.push("上下移动不该把选中项换掉");
if (!layerPanel?.hidContentRow) layerProblems.push("找不到有内容的图层行（layer_paint）");
if ((layerPanel?.paintedAfterHide || 0) !== 0 && (layerPanel?.paintedBefore || 0) > 0) {
  layerProblems.push(
    `隐藏有内容的图层后画面应变空（${layerPanel?.paintedBefore} → ${layerPanel?.paintedAfterHide}）`,
  );
}
if ((layerPanel?.paintedAfterShow || 0) !== (layerPanel?.paintedBefore || 0)) {
  layerProblems.push(
    `再显示之后画面应恢复（${layerPanel?.paintedBefore} → ${layerPanel?.paintedAfterShow}）`,
  );
}
if (!layerPanel?.lockedOnServer) layerProblems.push("点锁图标之后服务端应记录 locked=true");
console.log(`  【切层】from=${layerPanel?.switchedFrom} to=${layerPanel?.switchedTo} failed=${layerPanel?.switchFailed} rows=${JSON.stringify(layerPanel?.rowsAfterSwitch)} perLayer=${JSON.stringify(layerPanel?.objectsPerLayer)}`);
console.log("  【隐藏诊断】" + JSON.stringify(layerPanel?.hideDiag));
console.log(`  图层面板：${layerPanel?.rowCount} 行（自上而下 ✓ ${layerPanel?.firstRowIsTop ? "是" : "否"}）｜＋⇒${layerPanel?.afterAdd}｜⧉⇒${layerPanel?.afterDuplicate}（${layerPanel?.copyName}）｜上移生效 ${layerPanel?.moveChanged ? "✓" : "✗"}｜隐藏 ${layerPanel?.paintedBefore}→${layerPanel?.paintedAfterHide}→${layerPanel?.paintedAfterShow}｜锁定 ${layerPanel?.lockedOnServer ? "✓" : "✗"}（图标 ${layerPanel?.lockIcon}）`);
const layerShot = await capture("05-layer-panel");
console.log(`  截图：${layerShot || "（无）"}`);
for (const problem of layerProblems) problems.push(problem);

// **面板可见性与全屏画布** ✓（用户要求：左侧工具栏两列 + 可隐藏 ✓、右侧各窗口可隐藏 ✓、全屏画布 ✓）。
//
// 断言刻意读**计算样式与几何** ✓（`getComputedStyle` / `getBoundingClientRect` ✓），
// 而不是读我们自己加的类名 ✓ —— 读类名只能证明"JS 跑过了" ✗，
// 读样式才能证明"用户看到的确实变了" ✓（本项目吃过"类加上了但样式没生效"的亏 ✓）。
const panelsResult = await evaluate(`(async () => {
  const styles = (el) => (el ? getComputedStyle(el) : null);
  const visible = (el) => !!el && styles(el).display !== "none" && el.getBoundingClientRect().width > 0;
  const rail = document.getElementById("tools");
  const aside = document.querySelector("aside");
  const header = document.querySelector("header");
  const main = document.querySelector("main");
  const out = {};
  // ① **两列** ✓：栅格轨道数应为 2 ✓（读计算样式里的 grid-template-columns ✓）。
  const before = { railVisible: visible(rail), asideVisible: visible(aside) };
  out.railColumns = rail ? styles(rail).gridTemplateColumns.split(" ").filter(Boolean).length : 0;
  out.railButtons = rail ? rail.querySelectorAll("button").length : 0;
  // ② **隐藏左侧** ✓（用按钮而不是快捷键 ✓ ⇒ 顺带验证按钮接线 ✓）。
  document.getElementById("toggleRail").click();
  await new Promise((r) => setTimeout(r, 120));
  out.railHidden = !visible(rail);
  out.mainColumnsAfterRailHidden = styles(main).gridTemplateColumns.split(" ").filter(Boolean).length;
  out.asideStillVisible = visible(aside);
  document.getElementById("toggleRail").click();
  await new Promise((r) => setTimeout(r, 120));
  out.railBack = visible(rail);
  // ③ **隐藏右侧** ✓。
  document.getElementById("toggleDockers").click();
  await new Promise((r) => setTimeout(r, 120));
  out.asideHidden = !visible(aside);
  out.mainColumnsAfterDockersHidden = styles(main).gridTemplateColumns.split(" ").filter(Boolean).length;
  out.railStillVisible = visible(rail);
  document.getElementById("toggleDockers").click();
  await new Promise((r) => setTimeout(r, 120));
  out.asideBack = visible(aside);
  // ④ **全屏画布** ✓：两侧 + 头部都藏起来 ✓，画布仍在且**变大了** ✓。
  // **量"可用区"而不是画布本身** ✓：画布的 CSS 尺寸由**缩放**决定 ✓，
  // 全屏不会自动改变它 ✓（我第一版量 board ✗ ⇒ 断言"应更宽"失败 ✓，而全屏其实是对的 ✓）。
  // 真正应当变宽的是 **stage（可用区）** ✓。
  const area = () => document.querySelector(".stage")?.getBoundingClientRect().width || 0;
  const widthBefore = area();
  document.getElementById("toggleZen").click();
  await new Promise((r) => setTimeout(r, 200));
  out.zenRailHidden = !visible(rail);
  out.zenAsideHidden = !visible(aside);
  out.zenHeaderHidden = !visible(header);
  out.zenExitVisible = visible(document.getElementById("zenExit"));
  out.boardWidthBefore = Math.round(widthBefore);
  out.boardWidthZen = Math.round(area());
  out.zenPressed = document.getElementById("toggleZen")?.getAttribute("aria-pressed");
  out.before = before;
  return out;
})()`);

const panelProblems = [];
if (panelsResult?.railColumns !== 2) {
  panelProblems.push(`左侧工具栏应为**两列**（计算样式里读到 ${panelsResult?.railColumns} 条轨道）`);
}
if (panelsResult?.railButtons < 4) {
  panelProblems.push(`左侧工具栏按钮太少（${panelsResult?.railButtons} 个），看不出两列效果`);
}
if (!panelsResult?.railHidden) panelProblems.push("点击「◧ 工具栏」应隐藏左侧工具栏");
if (panelsResult?.mainColumnsAfterRailHidden !== 2) {
  panelProblems.push(`隐藏左栏后 main 应剩两列（实得 ${panelsResult?.mainColumnsAfterRailHidden}）`);
}
if (!panelsResult?.asideStillVisible) panelProblems.push("隐藏左栏**不该**连带隐藏右侧面板");
if (!panelsResult?.railBack) panelProblems.push("再点一次「◧ 工具栏」应恢复显示");
if (!panelsResult?.asideHidden) panelProblems.push("点击「◨ 面板」应隐藏右侧面板");
if (panelsResult?.mainColumnsAfterDockersHidden !== 2) {
  panelProblems.push(`隐藏右栏后 main 应剩两列（实得 ${panelsResult?.mainColumnsAfterDockersHidden}）`);
}
if (!panelsResult?.railStillVisible) panelProblems.push("隐藏右栏**不该**连带隐藏左侧工具栏");
if (!panelsResult?.asideBack) panelProblems.push("再点一次「◨ 面板」应恢复显示");
if (!panelsResult?.zenRailHidden || !panelsResult?.zenAsideHidden) {
  panelProblems.push("全屏画布应同时隐藏左右两侧");
}
if (!panelsResult?.zenHeaderHidden) panelProblems.push("全屏画布应隐藏头部");
if (!panelsResult?.zenExitVisible) {
  panelProblems.push("全屏时应有**退出把手**（否则用户不知道怎么出来）");
}
if ((panelsResult?.boardWidthZen || 0) <= (panelsResult?.boardWidthBefore || 0)) {
  panelProblems.push(
    `全屏后可用区应变宽（${panelsResult?.boardWidthBefore} ⇒ ${panelsResult?.boardWidthZen}）`,
  );
}
if (panelsResult?.zenPressed !== "true") {
  panelProblems.push("全屏时「⛶ 全屏」按钮应显示为已按下（aria-pressed）");
}

// **截图** ✓：正常布局 / 隐藏左 / 隐藏右 / 全屏 四张 ✓。
//
// **必须先退出全屏** ✗ —— 上面的断言结束时页面**还停在 zen 里** ✓，我第一版直接接着拍 ✓
// ⇒ `01-normal.png` 其实拍的是**全屏态** ✓（看截图才发现：头部没了、右上角是"退出全屏" ✓），
// 而且它与 `04-zen.png` **字节数完全相同** ✓ —— 这个"两张图一样大"其实早就提示了我 ✗。
// **教训** ✓：截图脚本的**状态前提**要和图名一致 ✓，否则图会骗人 ✓（比没有截图更糟 ✗）。
await evaluate(`document.getElementById("zenExit")?.click(); true`);
await new Promise((r) => setTimeout(r, 250));
const shotNormal = await capture("01-normal");
const shotRail = await evaluate(`document.getElementById("toggleRail").click(); true`) && (await new Promise((r) => setTimeout(r, 150)), await capture("02-rail-hidden"));
await evaluate(`document.getElementById("toggleRail").click(); true`);
await new Promise((r) => setTimeout(r, 120));
const shotDockers = await evaluate(`document.getElementById("toggleDockers").click(); true`) && (await new Promise((r) => setTimeout(r, 150)), await capture("03-dockers-hidden"));
await evaluate(`document.getElementById("toggleDockers").click(); true`);
await new Promise((r) => setTimeout(r, 120));
const shotZen = await evaluate(`document.getElementById("toggleZen").click(); true`) && (await new Promise((r) => setTimeout(r, 250)), await capture("04-zen"));
// **退出全屏** ✓ 并把状态复位 ✓（检查不能把用户的界面留在全屏里 ✓）。
await evaluate(`document.getElementById("zenExit").click(); true`);
await new Promise((r) => setTimeout(r, 200));
const zenExited = await evaluate(`getComputedStyle(document.querySelector("header")).display !== "none"`);
if (!zenExited) panelProblems.push("点击退出把手之后应回到普通布局");
console.log(`  面板：左侧工具栏 ${panelsResult?.railColumns} 列（原始：${panelsResult?.railColumnsRaw}｜display=${panelsResult?.railDisplay}）/ ${panelsResult?.railButtons} 个按钮｜隐藏左 ${panelsResult?.railHidden ? "✓" : "✗"}｜隐藏右 ${panelsResult?.asideHidden ? "✓" : "✗"}｜全屏 ${panelsResult?.zenHeaderHidden ? "✓" : "✗"}（可用区 ${panelsResult?.boardWidthBefore} → ${panelsResult?.boardWidthZen}）｜存储=${panelsResult?.persisted}`);
console.log(`  截图：${[shotNormal, shotRail, shotDockers, shotZen].filter(Boolean).join("、") || "（本机不支持截图）"}`);
for (const problem of panelProblems) problems.push(problem);

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
