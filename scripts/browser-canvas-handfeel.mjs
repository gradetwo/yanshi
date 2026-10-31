#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **★ 总超时守卫 ✗ ★**（2026-10-31）：**浏览器/CDP 起不来时本判据会**一直等**✗ ⇒
//   **∴ 超时**报「自己跑不了」并退 3** ✓（**与产品失败区分 ✓**）；`unref()` ⇒ **跑完不被它拖住** ✓。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 300000));
__judgeTimeout.unref();

// **画布手感与图层选择的浏览器验收** ✓ —— 三条都是**真实用户实测报告**的问题 ✓：
//
//   ① **滚轮/触摸板不再误触缩放** ✗（用户："web 画布很容易不小心就被放大缩小"✓，
//      要求"只有点击放大缩小键或输入具体数值才变化"✓）⇒ 滚轮现在**只平移** ✓；
//      **判据**：滚轮之后 `displayScale` **必须不变** ✓（接回缩放就红 ✓）；而视口**必须动** ✓（它得干活 ✓）。
//   ② **状态栏的百分比输入框能缩放** ✓（"输入具体数值"那个入口 ✓）；
//      **判据**：输入 200 ⇒ `displayScale` ≈ 2.0 ✓（即"1 文档像素显示成 2 个 CSS 像素"✓）。
//   ③ **新建图层后自动选中它** ✗（用户："新建图层后…图层列表看还是选中老的"✓）；
//      **判据**：既有 state（`state().layerId` ✓）**也有面板高亮**（`.layer-row.selected` ✓）——
//      只查 state 会漏掉这次的 bug ✓（旧代码 state 是对的 ✗、面板是老的 ✗）。
//   ④ 全程**零控制台错误** ✓。
//
// 用法：node scripts/browser-canvas-handfeel.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-handfeel";
if (!url) {
  console.error("用法: node scripts/browser-canvas-handfeel.mjs <viewer-url>");
  process.exit(2);
}
// **★ CDP 连接守卫 ✗ ★**（2026-10-16 ✓）：浏览器没起来时这里会抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
let list;
try {
  list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const viewerBase = url.split("?")[0];
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(viewerBase)) ||
  list.find((t) => t.type === "page");
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
    consoleLines.push((message.params.args || []).map((a) => a.value ?? "").join(" "));
  }
  if (message.method === "Runtime.exceptionThrown") {
    const details = message.params.exceptionDetails || {};
    consoleLines.push("exception: " + (details.exception?.description || details.text || ""));
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
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
// **★★★ 等待上限**必须可配置 ✗ ★★★**（**第 777 轮修；**CI 实测 ✓）
//   **∴ 症状 ✗**（**CI ✓）★**：**`⏱ 等待超时：WASM 内核就绪`**✗
//     ＋ **∴ 而**后续断言（**如**「设了 200% 但缩放仍在适配档」✓）**只是**后果** ✓ ★
//   **∴ 根因 ✗**：**上限写死**（**本仓 11 处：**6×30000／4×25000／1×20000 ✓）✗
//     ＋ **∴ 而** CI **的 runner **比开发机慢**✗ ⇒ **∴ 那些数字**都没有**产品依据** ✓ ★★
//   **∴ 修法 ✗**：**统一读 `YANSHI_WAIT_MS`**✗ ＋ **∴ 默认 120 秒** ✓ ★
//     ＋ **∴ 收益**：**判据测的是它真正要测的东西**（**产物是否就绪 ✓）而**不是**环境速度** ✓ ★
//       ＋ **∴ 代价**：**真异常时要**多等**✗（**∴ 但**只在**真异常时** ✓）★
const WAIT_MS = Number(process.env.YANSHI_WAIT_MS || 120000);
const waitFor = async (expression, label, timeoutMs = WAIT_MS) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await sleep(150);
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};
const capture = async (name) => {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const path = `${shotsDir}/${name}.png`;
  await fs.writeFile(path, Buffer.from(data, "base64"));
  return path;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true }); // **必须忽略缓存** ✓（见 scripts/README.md ✓）
// ⚠️ **这里不需要固定睡眠** ✗（第 896 轮 ✓，**类别③：等待多余 ⇒ 直接去掉** ✓）：
// **下一行已经在正确地等** ✓ —— `waitFor(… "查看器就绪")` ✓ 并且**等不到就 `exit(3)`** ✓
//（**"等不到要有自己的失败信息"** ✓，正是要提倡的形态 ✓）⇒ 1.6 秒纯属多余 ✓，删掉它。
if (!(await waitFor("typeof window.yanshi === 'object' && !!document.getElementById('board')", "查看器就绪"))) {
  process.exit(3);
}

// ① 滚轮只平移、不缩放 ✓
const beforeWheel = await evaluate(`(() => {
  const s = window.yanshi.state();
  return { scale: s.zoom, userZoom: s.userZoom, viewport: s.viewport || null };
})()`);
await evaluate(`(() => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  board.dispatchEvent(new WheelEvent("wheel", {
    bubbles: true, cancelable: true, deltaY: -600,
    clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2,
  }));
})()`);
await sleep(500);
const afterWheel = await evaluate(`(() => {
  const s = window.yanshi.state();
  return { scale: s.zoom, userZoom: s.userZoom, viewport: s.viewport || null };
})()`);
const panned =
  beforeWheel.viewport && afterWheel.viewport
    ? Math.abs(afterWheel.viewport.x - beforeWheel.viewport.x) +
      Math.abs(afterWheel.viewport.y - beforeWheel.viewport.y)
    : null;
console.log(
  `  ① 滚轮：displayScale ${beforeWheel.scale} → ${afterWheel.scale}` +
    `（用户缩放 ${beforeWheel.userZoom} → ${afterWheel.userZoom}，视口位移 ${panned === null ? "?" : panned}）`,
);
// **判据落在"用户缩放"上** ✓：`displayScale = fit × userZoom` ✓ —— 而 `fit` 会随**可用区域**变
//（换行/状态栏高度/窗口尺寸 ✓）⇒ 拿 `displayScale` 当判据会把"布局重排"误判成"滚轮缩放了" ✗。
if (afterWheel.userZoom !== beforeWheel.userZoom) {
  console.error("❌ 滚轮改变了缩放 ⇒ 触摸板/滚轮误触又会放大缩小 ✗");
  await capture("wheel-zoomed-failed");
  process.exit(1);
}
const shotWheel = await capture("wheel-pans-not-zooms");
// **放大之后再滚一次** ✓：`fit` 时整幅可见 ⇒ 视口**无处可移** ✓（位移 0 是正确行为 ✗ 不是 bug ✓）。
// 所以"滚轮会平移"这条判据必须在**放得下不去**的尺度上量 ✓。

// ② 状态栏百分比输入框能缩放 ✓
// **② 之前必须先等布局稳定** ✓ —— displayScale = fit × userZoom ✓，而 fit 随可用区域变 ✓：
//   实测失败（main run 40836360543，head 6b91fca2）：布局未定型时输入 300% ⇒ handler 按旧 fit 0.5879
//   算出 userZoom = 3/0.5879 = 5.103 ✓，随后文档载入/布局定型触发 clampViewport ⇒ fit 长到 2.4667 ⇒
//   displayScale = 5.103 × 2.4667 = 12.587 ✗，输入框自己都回填成 1259% ✗ —— 断言 (2.5, 3.6) 于是红 ✓。
//   ∴ 输入之前先等「舞台矩形＋文档尺寸＋displayScale」两次读取（间隔 300ms）全等 ✓；
//   超时（8 秒）不跳过：按原流程继续断言（退化为旧行为，不静默丢覆盖 ✓），断言本身仍是裁判 ✓。
const layoutSnapshot = async () =>
  evaluate(`(() => {
    const b = document.getElementById("board");
    if (!b) return undefined;
    const r = b.getBoundingClientRect();
    const s = window.yanshi.state();
    const d = s.docSize || {};
    return [Math.round(r.width), Math.round(r.height), d.w ?? null, d.h ?? null, s.zoom].join("|");
  })()`);
{
  let prev;
  let stable = false;
  const until = Date.now() + 8000;
  while (Date.now() < until) {
    const cur = await layoutSnapshot();
    if (cur !== undefined && cur === prev) {
      stable = true;
      break;
    }
    if (cur !== undefined) prev = cur;
    await sleep(300);
  }
  if (!stable) console.log("  ② 前置：布局 8 秒内未稳定，仍按原流程继续断言（超时不是产品失败，断言仍是裁判）");
}
await evaluate(`(() => {
  const box = document.getElementById("zoomInput");
  box.value = "300";
  box.dispatchEvent(new Event("change", { bubbles: true }));
})()`);
await waitFor("window.yanshi.state().zoom > 2.5", "缩放输入生效", 8000);
const afterInput = await evaluate("window.yanshi.state().zoom");
const zoomDiag = await evaluate(`(() => {
  const s = window.yanshi.state();
  return { zoom: s.zoom, userZoom: s.userZoom, box: (document.getElementById("zoomInput") || {}).value,
           label: (document.getElementById("zoom") || {}).textContent,
           board: (() => { const r = document.getElementById("board").getBoundingClientRect();
                           return { w: Math.round(r.width), h: Math.round(r.height) }; })(),
           doc: s.docSize || null };
})()`);
console.log("  ② 诊断：" + JSON.stringify(zoomDiag));
console.log(`  ② 输入 300% ⇒ displayScale ${afterInput}`);
if (!(afterInput > 2.5 && afterInput < 3.6)) {
  console.error(`❌ 输入 300% 没有把显示比例调到约 3.0（实测 ${afterInput}）`);
  await capture("zoom-input-failed");
  process.exit(1);
}

// ①b **放大之后再滚一次滚轮：用户缩放仍然必须不变** ✓。
//     **判据只用 `userZoom`** ✓ —— 实测发现 `displayScale` 会**自己变** ✗
//     （`displayScale = fit × userZoom` ✓，而 `fit` 随**可用区域**变 ✓：
//      同一页面里实测 board 从 344×311 变成 300×150 ✗ ⇒ 画布会**自己缩放** ✓）。
//     ⇒ 那是**另一件事**（很可能正是用户说的"每一笔结束能感受到画布抖动" ✗），
//       本轮**不当成"滚轮缩放"的证据** ✗，如实记下 ✓（下一步：把画布舞台尺寸从面板内容里解耦 ✓）。
const viewportBeforePan = await evaluate("window.yanshi.state()");
await evaluate(`(() => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  board.dispatchEvent(new WheelEvent("wheel", {
    bubbles: true, cancelable: true, deltaY: 220,
    clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2,
  }));
})()`);
await sleep(700);
const afterPan = await evaluate("window.yanshi.state()");
console.log(
  `  ①b 放大后滚轮：userZoom ${viewportBeforePan.userZoom} → ${afterPan.userZoom}` +
    `（displayScale ${viewportBeforePan.zoom} → ${afterPan.zoom} —— 后者会被布局重排带动 ✗）`,
);
if (afterPan.userZoom !== viewportBeforePan.userZoom) {
  console.error("❌ 放大之后滚轮又改了**用户缩放** ✗");
  process.exit(1);
}
const shotPan = await capture("wheel-noop-zoomed");

// ③ 新建图层 ⇒ **状态与面板高亮都必须落到新图层** ✓
const beforeLayer = await evaluate("window.yanshi.state().layerId");
const rowsBefore = await evaluate("document.querySelectorAll('.layer-row').length");
await evaluate("document.getElementById('layerAdd').click()");
// **等面板真的多出一行** ✓（不是睡固定时长 ✗ —— 那会在慢机器上冤判 ✓）。
await waitFor(`document.querySelectorAll('.layer-row').length > ${rowsBefore}`, "图层行增加", 8000);
const logTail = await evaluate(`(() => {
  const lines = Array.from(document.querySelectorAll("#log .line, #log div"));
  return lines.slice(-2).map((line) => line.textContent).join(" | ").slice(0, 160);
})()`);
console.log(`  ③ 面板行数 ${rowsBefore} ⇒ ${await evaluate("document.querySelectorAll('.layer-row').length")}｜日志：${logTail}`);
const layerState = await evaluate(`(() => {
  const rows = Array.from(document.querySelectorAll(".layer-row"));
  const selected = rows.find((row) => row.classList.contains("selected"));
  return {
    current: window.yanshi.state().layerId,
    selected: selected ? selected.dataset.layerId || null : null,
    rows: rows.length,
  };
})()`);
console.log(
  `  ③ 新建图层：之前 ${beforeLayer} ⇒ 现在 state=${layerState.current}、` +
    `面板高亮=${layerState.selected}（共 ${layerState.rows} 行）`,
);
if (!layerState.current || layerState.current === beforeLayer) {
  console.error("❌ 新建之后没有切到新图层（state 没变）");
  process.exit(1);
}
if (layerState.selected !== layerState.current) {
  console.error("❌ 面板高亮与当前图层不一致 ⇒ 用户会以为「在新层上画」，其实画在旧层 ✗");
  await capture("layer-selection-failed");
  process.exit(1);
}
const shotLayer = await capture("new-layer-selected");

// ⑤ **画一笔之后，用户缩放不许变** ✓（用户："每一笔结束能感受到画布的一个更新和抖动"✗）。
//    真因：预览图加载完成的那个回调用**无条件** `state.zoom = 1` ✓ ⇒ 画布自己缩放回整幅 ✗。
//    **判据**：先放大到 300% ✓，用**真实拖动**画一笔 ✓，然后 `userZoom` **必须一动不动** ✓
//    （`displayScale` 只作诊断 ✓ —— `displayScale = fit × userZoom` ✓，而 `fit` 会随布局重排动 ✗，
//     把它当判据会冤判成"缩放变了" ✗；这一点本轮实测过 ✓）。
const beforeStroke = await evaluate("window.yanshi.state()");
await evaluate(`(async () => {
  const board = document.getElementById("board");
  const rect = board.getBoundingClientRect();
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, point) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 41, pointerType: "mouse",
    isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
  }));
  fire("pointerdown", at(0.3, 0.45));
  await new Promise((r) => setTimeout(r, 60));
  for (const fx of [0.4, 0.5, 0.6]) { fire("pointermove", at(fx, 0.45)); await new Promise((r) => setTimeout(r, 60)); }
  fire("pointerup", at(0.6, 0.45));
})()`);
await sleep(2500); // 落笔 + 提交 + 预览（**这段正是原来抖动的窗口** ✓）
const afterStroke = await evaluate("window.yanshi.state()");
console.log(
  `  ⑤ 画一笔后：userZoom ${beforeStroke.userZoom} → ${afterStroke.userZoom}` +
    `（displayScale ${beforeStroke.zoom} → ${afterStroke.zoom} —— 仅诊断 ✓）`,
);
if (afterStroke.userZoom !== beforeStroke.userZoom) {
  console.error("❌ 画一笔把用户缩放改掉了 ⇒ 那就是「每笔抖一下」的根 ✗");
  await capture("stroke-zoom-reset-failed");
  process.exit(1);
}
const shotStroke = await capture("stroke-keeps-zoom");

// ④ 控制台
const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ④ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 6).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      wheel: { before: beforeWheel, after: afterWheel, panned, screenshot: shotWheel },
      wheelPan: { moved: panned, screenshot: shotPan },
      zoomInput: { displayScale: afterInput },
      layer: { before: beforeLayer, ...layerState, screenshot: shotLayer },
      strokeKeepsZoom: {
        before: beforeStroke.userZoom,
        after: afterStroke.userZoom,
        beforeScale: beforeStroke.zoom,
        afterScale: afterStroke.zoom,
        screenshot: shotStroke,
      },
      consoleErrors: errors.length,
    },
    null,
    2,
  ),
);
ws.close();
