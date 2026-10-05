#!/usr/bin/env node
// **画笔区可折叠 + 调色板/纹理浮层入口（点击或快捷键）+ 用完即收** ✓ —— 用户第 4 条 ✓。
//
// **判据（都能红 ✓）**：
//   ① 折叠：折起来之后所有画笔控件必须**看不见** ✓，展开又回来 ✓；**而且选好的笔与颜色不能变** ✓
//      （"折叠"只是隐藏 ✗，不许顺手把状态重置 ✗）；
//   ② 快捷键：`P` ⇒ 浮层打开且**聚焦到调色板**卡 ✓；`T` ⇒ 聚焦到纹理 ✓；`Esc` ⇒ 收起 ✓
//      （按了没反应 ⇒ 红 ✓）；
//   ③ 用完即收：勾着「用完即收」时点一个色块 ⇒ 浮层**收起** ✓；
//      取消勾选再点 ⇒ 浮层**仍然开着** ✓（这一半才是"开关真的管事"的判据 ✓）；
//   ④ 零控制台错误 ✓。
//
// 用法：node scripts/browser-brush-panel.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-brush-panel";
if (!url) {
  console.error("用法: node scripts/browser-brush-panel.mjs <viewer-url>");
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
const pressKey = (key) =>
  evaluate(`(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: ${JSON.stringify(key)}, bubbles: true }));
    return true;
  })()`);

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
// ⚠️ **这里不需要固定睡眠** ✗（第 898 轮 ✓，**类别③：等待多余 ⇒ 直接去掉** ✓）：
// **下一行（`waitFor(… "查看器就绪")`）已经在正确地等** ✓ 且**等不到就 `exit(3)`** ✓。
// ⚠️ **注意顺序未被改变** ✓：中间的 `consoleLines.length = 0` 仍**先于** `waitFor` ✓ ⇒
//    "清空控制台后才开始等就绪"这个既有行为**与本轮改动无关** ✓（**原先如此 ✓**）。
consoleLines.length = 0; // 从这里开始才算我们的（浏览器里可能还挂着上一次的页面 ✓）
if (!(await waitFor("typeof window.yanshiBrushArea === 'object' && typeof window.yanshiDock === 'object'", "查看器就绪"))) {
  process.exit(3);
}

// ① 折叠：控件看不见，但**选好的东西不许变** ✓
const before = await evaluate(`(() => ({
  brush: document.getElementById("brush").value,
  color: document.getElementById("color").value,
  smooth: document.getElementById("smooth").checked,
}))()`);
const collapse = await evaluate(`(() => {
  window.yanshiBrushArea.setCollapsed(true);
  const controls = Array.from(document.querySelectorAll(".brush-control"));
  const hidden = controls.filter((node) => node.offsetParent === null).length;
  return { controls: controls.length, hidden, collapsed: window.yanshiBrushArea.collapsed() };
})()`);
const after = await evaluate(`(() => ({
  brush: document.getElementById("brush").value,
  color: document.getElementById("color").value,
  smooth: document.getElementById("smooth").checked,
}))()`);
await evaluate("window.yanshiBrushArea.setCollapsed(false)");
const expanded = await evaluate(`(() => {
  const controls = Array.from(document.querySelectorAll(".brush-control"));
  return { visible: controls.filter((node) => node.offsetParent !== null).length, controls: controls.length };
})()`);
console.log(
  `  ① 折叠：${collapse.hidden}/${collapse.controls} 个控件隐藏；展开后 ${expanded.visible}/${expanded.controls} 可见`,
);
if (!(collapse.controls > 5 && collapse.hidden === collapse.controls)) {
  console.error("❌ 折叠之后仍有画笔控件可见 ⇒「可隐藏」没做到 ✗");
  await capture("brush-area-collapse-failed");
  process.exit(1);
}
if (expanded.visible !== expanded.controls) {
  console.error("❌ 展开之后控件没全回来 ✗");
  process.exit(1);
}
if (JSON.stringify(before) !== JSON.stringify(after)) {
  console.error(`❌ 折叠把状态改了 ✗（之前 ${JSON.stringify(before)}、之后 ${JSON.stringify(after)}）`);
  process.exit(1);
}
const shotCollapsed = await capture("brush-area-collapsed");

// ② 快捷键：P 调色板 / T 纹理 / Esc 收起 ✓
await pressKey("p");
await sleep(300);
const afterP = await evaluate(`(() => ({
  open: window.yanshiDock.isOpen(),
  palette: !!document.querySelector("#cardPalette.asset-dock-target"),
  texture: !!document.querySelector("#cardTexture.asset-dock-target"),
}))()`);
await pressKey("Escape");
await sleep(250);
const afterEsc = await evaluate("window.yanshiDock.isOpen()");
await pressKey("t");
await sleep(300);
const afterT = await evaluate(`(() => ({
  open: window.yanshiDock.isOpen(),
  texture: !!document.querySelector("#cardTexture.asset-dock-target"),
}))()`);
console.log(
  `  ② 快捷键：P ⇒ 开=${afterP.open} 调色板高亮=${afterP.palette}；Esc ⇒ 开=${afterEsc}；` +
    `T ⇒ 开=${afterT.open} 纹理高亮=${afterT.texture}`,
);
if (!afterP.open || !afterP.palette) {
  console.error("❌ 按 P 没有把调色板浮出来 ✗");
  await capture("shortcut-palette-failed");
  process.exit(1);
}
if (afterEsc) {
  console.error("❌ 按 Esc 没有收起浮层 ✗");
  process.exit(1);
}
if (!afterT.open || !afterT.texture) {
  console.error("❌ 按 T 没有把纹理浮出来 ✗");
  await capture("shortcut-texture-failed");
  process.exit(1);
}
const shotDock = await capture("dock-texture-focused");

// ③ 用完即收（默认勾上）⇒ 取色之后必须收起；取消勾选 ⇒ 不许收 ✓
await evaluate("window.yanshiDock.open()");
await waitFor("document.querySelectorAll('#paletteSwatches button').length > 0", "调色板色块", 20000);
const autoCloseOn = await evaluate("window.yanshiDock.autoClose()");
const closedAfterPick = await evaluate(`(() => {
  const chip = document.querySelector("#paletteSwatches button");
  chip.click();
  return !window.yanshiDock.isOpen();
})()`);
await evaluate("window.yanshiDock.open()");
const staysOpen = await evaluate(`(() => {
  window.yanshiDock.setAutoClose(false);
  const chips = document.querySelectorAll("#paletteSwatches button");
  chips[chips.length - 1].click();
  return window.yanshiDock.isOpen();
})()`);
console.log(
  `  ③ 用完即收：默认勾选=${autoCloseOn}｜取色后收起=${closedAfterPick}｜取消勾选后取色仍开着=${staysOpen}`,
);
if (!autoCloseOn || !closedAfterPick) {
  console.error("❌ 勾着「用完即收」时取完色浮层没收起 ✗");
  await capture("dock-autoclose-failed");
  process.exit(1);
}
if (!staysOpen) {
  console.error("❌ 取消勾选之后取色还是把浮层收走了 ⇒ 那个开关不管事 ✗");
  process.exit(1);
}
await evaluate("window.yanshiDock.setAutoClose(true)");
const shotFinal = await capture("dock-after-pick");

const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ④ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 4).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      collapse: { controls: collapse.controls, hidden: collapse.hidden, visibleAfterExpand: expanded.visible },
      shortcuts: { p: afterP, t: afterT, escClosed: afterEsc },
      autoClose: { defaultOn: autoCloseOn, closedAfterPick, staysOpenWhenOff: staysOpen },
      screenshots: [shotCollapsed, shotDock, shotFinal],
    },
    null,
    2,
  ),
);
ws.close();
