#!/usr/bin/env node
// **右栏 tab 化 + 整页垂直锁死 + 画布内部滚动** ✓ —— 用户第 5 条 ✓。
//
// **判据（都能红 ✓）**：
//   ① 页面本身**不滚** ✓，而右栏**自己滚** ✓（"垂直方向锁死、面板内部滚"✓）；
//   ② 切 tab：只有那一个窗格的卡可见 ✓，而且**卡片总数不许变** ✓
//      （17 张卡搬完还是 17 张 ✓ —— "搬着搬着丢了一张"是本项目的老病 ✓，所以单独判 ✓）；
//   ③ 放大到 400% ⇒ **舞台内部**出现横向滚动 ✓（不封顶才有这个 ✓）；回到 100% ⇒ 又不滚 ✓；
//   ④ **浮层与 tab 共存** ✓：浮出的调色板/纹理卡，收起后必须回到**素材窗格里** ✓
//      （搬回旧父节点 ⇒ 红 ✓ —— 这就是"搬卡顺序"那条取舍的判据 ✓）；
//   ⑤ 零控制台错误 ✓。
//
// 用法：node scripts/browser-layout.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-layout";
if (!url) {
  console.error("用法: node scripts/browser-layout.mjs <viewer-url>");
  process.exit(2);
}
const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
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
const waitFor = async (expression, label, timeoutMs = 25000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await sleep(200);
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
await send("Page.reload", { ignoreCache: true });
await sleep(1600);
consoleLines.length = 0;
if (!(await waitFor("typeof window.yanshiRightTabs === 'object' && typeof window.yanshiDock === 'object'", "查看器就绪"))) {
  process.exit(3);
}
// **等画布真的被设过尺寸再量** ✗ —— 第 47 轮就是在这里栽的 ✓：启动瞬间 `<canvas>` 还是默认的
// 300×150 ✓、`style.width` 为空 ✓ ⇒ 我据此得出"尺寸没落到元素上"✗，而真相只是"还没轮到它"✓。
// ⇒ 判据必须**等被测对象稳定** ✓（`board.width` 走出默认值 ✓ + 至少补过一次服务端像素 ✓）。
// **不要用写死的像素阈值** ✗（第 430 轮 ✓）：`board.width` 是**位图**宽度 ✓，它按
// **显示尺寸 × 设备缩放** 来设 ✓（实测：`style.width = "424px"` 而 `board.width = 320` ✓，
// 比值 0.755 就是缩放 ✓）⇒ 拿它去卡一个"CSS 像素"的常数（原来的 `> 400` ✗）会在缩放 ≠ 1 时**假红** ✓。
// ⇒ 改成与**该元素自己的显示宽度**比较 ✓（缩放无关 ✓），并保留"不是默认 300×150"这一层 ✓。
if (!(await waitFor(
  `(() => {
     const b = document.getElementById("board");
     const shown = b.getBoundingClientRect().width;
     return b.width !== 300 && b.width >= Math.floor(shown * 0.5)
            && window.yanshi.state().serverBlits > 0;
   })()`,
  "画布尺寸与首帧",
  20000,
))) {
  // **两个条件各自的值都打出来** ✓（第 422 轮 ✓）：这条等待是**两个条件相与** ✗ ——
  // `board.width > 400` ✓ **且** `serverBlits > 0` ✓ ⇒ 任一边没成立都会在这里退出 ✓，
  // 而原消息只描述前者的症状 ✗ ⇒ **看不出是哪一边** ✓（**失败理由无法定位** ✗）。
  const diag = await evaluate(`(() => {
    const b = document.getElementById("board");
    let blits = "?";
    try { blits = window.yanshi.state().serverBlits; } catch (e) { blits = "取不到: " + e; }
    return { width: b && b.width, height: b && b.height, styleWidth: b && b.style.width, serverBlits: blits };
  })()`);
  console.error(`❌ 画布尺寸与首帧没就绪 ⇒ board.width=${diag.width}（应与显示宽度同量级、且不是默认 300 ✓）显示宽度=${JSON.stringify(diag.styleWidth)} height=${diag.height} serverBlits=${diag.serverBlits}（需 >0 ✓）`);
  process.exit(1);
}

// ① 页面不滚、右栏自己滚 ✓
const scroll = await evaluate(`(() => {
  const aside = document.querySelector("aside");
  const stage = document.querySelector(".stage");
  // **判据是「真去滚一下看动不动」** ✗ —— scrollHeight 即使 overflow:hidden 也会报**内容高度** ✓
  //（第一版拿它当判据 ⇒ 误报"整页还能滚"✗，而 「documentElement」 那一项其实是 0 ✓）。
  const before = window.scrollY;
  window.scrollTo(0, 600);
  const moved = window.scrollY - before;
  window.scrollTo(0, before);
  return {
    pageScrollMoved: moved,
    docOverflow: document.documentElement.scrollHeight - window.innerHeight,
    asideScrolls: aside.scrollHeight - aside.clientHeight,
    stageOverflowX: stage ? stage.scrollWidth - stage.clientWidth : null,
  };
})()`);
console.log(
  `  ① 滚动：试着往下滚 ${scroll.pageScrollMoved}px（0 = 锁死 ✓）；文档多出 ${scroll.docOverflow}px；` +
    `右栏内部可滚 ${scroll.asideScrolls}px`,
);
if (scroll.pageScrollMoved !== 0 || scroll.docOverflow > 2) {
  console.error("❌ 整页还能滚 ⇒「垂直方向锁死」没做到 ✗");
  await capture("layout-page-scrolls");
  process.exit(1);
}

// ② 切 tab：只有那一个窗格可见，且**卡片总数不变** ✓
const tabStates = {};
// **声明必须在循环外** ✓（第 440 轮 ✓）：原先把结果声明在循环体里 ✗
// ⇒ `const` 是块级作用域 ✓ ⇒ 循环一结束它就没了 ✓ ⇒ 下面汇总里再用 ⇒ **ReferenceError** ✗
//（这个错一直没露头 ✓ —— **上面的 `waitFor` 超时会先 `process.exit(1)`** ✗ ⇒ **一处红遮住下一处** ✓）。
let movedOut = null; // 循环内赋值 ✓；循环外仍可见 ✓（取最后一次迭代的值 ✓，与汇总语义一致 ✓）
for (const key of ["history", "assets", "file", "diag", "paint"]) {
  const state = await evaluate(`(() => { window.yanshiRightTabs.show(${JSON.stringify(key)}); return window.yanshiRightTabs.state(); })()`);
  tabStates[key] = state;
  const total = state.counts.reduce((sum, item) => sum + item.cards, 0);
  console.log(
    `  ② tab「${key}」⇒ 可见卡 ${state.visibleCards} 张（该窗格 ${state.counts.find((c) => c.key === key).cards} 张，全部合计 ${total} 张）`,
  );
  if (state.active !== key) {
    console.error(`❌ 切到「${key}」之后活动窗格是「${state.active}」✗`);
    process.exit(1);
  }
  if (state.visibleCards !== state.counts.find((c) => c.key === key).cards) {
    console.error(`❌「${key}」窗格里可见卡数与应有卡数不一致 ⇒ 有卡没挂上/没藏起来 ✗`);
    await capture("layout-tab-mismatch");
    process.exit(1);
  }
  // **16 张** ✓ = 原来的 17 张减去**工程包那张** —— 它按用户第 6 条的要求被搬进了**顶栏「文件」菜单** ✓
  //（"导入导出不适合放在信息面板" ✓）。少的那张必须**在菜单里找得到** ✓，所以这里同时点名 ✓。
  if (total !== 16) {
    console.error(`❌ 面板里的卡片总数变成 ${total} ⇒ 搬卡时丢了卡 ✗（应当是 16 张 ✓）`);
    await capture("layout-lost-card");
    process.exit(1);
  }
  movedOut = await evaluate(`(() => {
    const ids = ["newDoc", "openDoc", "exportPng", "projectExport", "projectImport"];
    const menu = document.getElementById("fileMenuBody");
    return {
      inMenu: ids.filter((id) => menu && menu.contains(document.getElementById(id))),
      inAside: ids.filter((id) => document.querySelector("aside") && document.querySelector("aside").contains(document.getElementById(id))),
      fileTabHidden: (() => { const tab = document.querySelector('#rightTabs button[data-tab="file"]');
        return tab ? tab.hidden : null; })(),
    };
  })()`);
  if (movedOut.inAside.length > 0) {
    console.error(`❌ 信息面板里还留着 ${movedOut.inAside.join(", ")} ⇒ 没搬出去 ✗`);
    process.exit(1);
  }
  if (movedOut.inMenu.length !== 5) {
    console.error(`❌ 只搬进菜单 ${movedOut.inMenu.length}/5 个控件 ⇒ 有控件在搬运中丢了 ✗`);
    await capture("layout-move-lost-control");
    process.exit(1);
  }
}
await evaluate("window.yanshiRightTabs.show('assets')");
const shotTabs = await capture("layout-tabs-assets");

// ③ 放大 ⇒ **画布比舞台宽、且舞台能滚** ✓；回到 100% ⇒ 画布正好塞满 ✓。
//    **量画布本身，别量 `.stage` 的 scrollWidth** ✗ —— 那里恒有 11px 的差（另有子元素 ✓），
//    100% 与 400% 都是 11px ✓ ⇒ 那是我**量错了对象** ✗（连续第三次判据错，教训同型 ✓）。
const zoomTo = async (percent) => {
  await evaluate(`(() => {
    const box = document.getElementById("zoomInput");
    box.value = ${JSON.stringify(String(percent))};
    box.dispatchEvent(new Event("change", { bubbles: true }));
  })()`);
  await sleep(800);
};
const metrics = () =>
  evaluate(`(() => {
    const stage = document.querySelector(".stage");
    const board = document.getElementById("board");
    const width = Math.round(board.getBoundingClientRect().width);
    const client = stage.clientWidth;
    const before = stage.scrollLeft;
    stage.scrollLeft = 60;
    const canScroll = stage.scrollLeft !== before;
    stage.scrollLeft = before;
    return {
      boardWidth: width,
      stageClient: client,
      overflow: width - client,
      canScroll,
      pageStillLocked: document.documentElement.scrollHeight - window.innerHeight <= 2,
    };
  })()`);
await zoomTo(400);
const zoomed = await metrics();
// **"正好塞满"要按「适配」量** ✗ —— 输入框里的 `100%` 意思是 **1:1**（1 文档像素 = 1 CSS 像素 ✓），
// 那时画布 900px 比舞台宽 **正是对的** ✓（连续第四次判据错，教训同型 ✓：先问判据在问什么 ✓）。
await evaluate(`document.getElementById("zoomFit").click()`);
await sleep(800);
const fitted = await metrics();
console.log(
  `  ③ 缩放：400% ⇒ 画布 ${zoomed.boardWidth}px vs 舞台 ${zoomed.stageClient}px（溢出 ${zoomed.overflow}，能滚=${zoomed.canScroll}）` +
    `；适配 ⇒ 画布 ${fitted.boardWidth}px vs 舞台 ${fitted.stageClient}px（溢出 ${fitted.overflow}）`,
);
if (!(zoomed.overflow > 10) || !zoomed.canScroll) {
  console.error("❌ 放大之后画布没有超出舞台 / 舞台滚不动 ⇒「画布内部独立滚动」没做到 ✗");
  await capture("layout-no-stage-scroll");
  process.exit(1);
}
if (!zoomed.pageStillLocked) {
  console.error("❌ 放大把整页顶开了 ⇒ 垂直锁死被破坏 ✗");
  process.exit(1);
}
if (fitted.overflow > 2) {
  console.error(`❌ 回到 100% 之后画布仍比舞台宽 ${fitted.overflow}px ⇒ fit 不再"正好塞满" ✗`);
  process.exit(1);
}
const shotZoom = await capture("layout-zoomed-stage-scrolls");

// ④ 浮层与 tab 共存：收起之后卡必须回到**素材窗格** ✓
const dockFlow = await evaluate(`(() => {
  const where = (id) => {
    const node = document.getElementById(id);
    const parent = node && node.parentNode;
    return parent ? (parent.id || parent.dataset.pane || parent.className || parent.tagName) : null;
  };
  window.yanshiRightTabs.show("assets");
  const before = { palette: where("cardPalette"), texture: where("cardTexture") };
  window.yanshiDock.open();
  const open = { palette: where("cardPalette"), texture: where("cardTexture") };
  window.yanshiDock.close();
  const after = { palette: where("cardPalette"), texture: where("cardTexture") };
  return { before, open, after, pane: document.querySelector('.tab-pane[data-pane="assets"]') ? "assets" : null };
})()`);
console.log(
  `  ④ 浮层：浮出前 ${dockFlow.before.palette}/${dockFlow.before.texture}` +
    ` ⇒ 浮出时 ${dockFlow.open.palette}/${dockFlow.open.texture}` +
    ` ⇒ 收起后 ${dockFlow.after.palette}/${dockFlow.after.texture}`,
);
if (dockFlow.open.palette !== "assetDockBody" || dockFlow.open.texture !== "assetDockBody") {
  console.error("❌ 点了「素材」之后卡没搬进浮层 ✗");
  process.exit(1);
}
if (dockFlow.after.palette !== "assets" || dockFlow.after.texture !== "assets") {
  console.error("❌ 收起之后卡没回到「素材」窗格 ⇒ 它们跑到别处去了 ✗（搬卡顺序那条取舍的判据 ✓）");
  await capture("layout-dock-homes-wrong");
  process.exit(1);
}
const shotDock = await capture("layout-dock-and-tabs");

const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ⑤ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 4).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      scroll,
      tabs: Object.fromEntries(Object.entries(tabStates).map(([key, value]) => [key, value.visibleCards])),
      movedToFileMenu: movedOut,
      stageScroll: { zoomed: zoomed.overflow, fitted: fitted.overflow, canScroll: zoomed.canScroll, pageStillLocked: zoomed.pageStillLocked },
      dockHomes: dockFlow,
      screenshots: [shotTabs, shotZoom, shotDock],
    },
    null,
    2,
  ),
);
ws.close();
