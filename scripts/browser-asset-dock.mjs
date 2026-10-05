#!/usr/bin/env node
// **素材浮层**的浏览器验收 ✓（用户："画笔区快捷方式、点开浮出来" ✓）。
//
// 判据（每条都能红 ✓）：
//   ① 打开前，两张卡在**它们原来的父节点**里 ✓（记下来 ✓）；
//   ② 点工具条上的「素材」⇒ 两张卡**搬进浮层** ✓、浮层可见 ✓；
//   ③ **搬过去还能用** ✓：点浮层里的色块 ⇒ **笔刷色真的变了** ✓
//      —— 这条正是"上一版把面板弄空 / 交互失效"的**反面** ✓（搬的是**节点本身** ✓，不是重建 ✓）；
//   ④ 点「收起」⇒ 两张卡**回到原来的父节点、顺序不变** ✓、浮层隐藏 ✓；
//   ⑤ 全程**零控制台错误** ✓。
//
// 用法：node scripts/browser-asset-dock.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-asset-dock";
if (!url) {
  console.error("用法: node scripts/browser-asset-dock.mjs <viewer-url>");
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
    consoleLines.push("exception: " + (message.params.exceptionDetails?.exception?.description || ""));
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
const waitFor = async (expression, label, timeoutMs = 30000) => {
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
// **强制重新取页面** ✓（旧页面会让探针看到旧行为 ✓，见 scripts/README.md ✓）。
await send("Page.reload", { ignoreCache: true });
// **睡眠已删** ✓（第 899 轮 ✓）：下一行 `waitFor` 已在正确地等 ✓，且**等不到会 exit** ✓。
if (!(await waitFor("typeof window.yanshiDock === 'object'", "浮层入口就绪"))) process.exit(3);
if (!(await waitFor("document.querySelectorAll('#paletteSwatches button').length > 0", "调色板色块装载"))) {
  process.exit(3);
}

// ① 打开前：卡在原位 ✓
const before = await evaluate(`(() => ({
  where: window.yanshiDock.where(),
  homes: window.yanshiDock.homes(),
  isOpen: window.yanshiDock.isOpen(),
  chips: document.querySelectorAll('#paletteSwatches button').length,
}))()`);
console.log(`  ① 打开前：open=${before.isOpen} 卡的位置=${JSON.stringify(before.where)} 色块=${before.chips}`);
if (before.isOpen || before.where.includes("assetDockBody")) {
  console.error("❌ 还没点就浮出来了（或者卡片本来就在浮层里）");
  process.exit(1);
}
if (before.chips < 2) {
  console.error("❌ 调色板色块太少，后面那条「搬过去还能用」就没法证明");
  process.exit(1);
}

// ② 点「素材」⇒ 卡片搬进去 ✓
await evaluate("document.getElementById('assetFloat').click()");
if (!(await waitFor("window.yanshiDock.isOpen()", "浮层打开"))) process.exit(1);
const opened = await evaluate(`(() => ({
  where: window.yanshiDock.where(),
  visible: !document.getElementById('assetDock').hidden,
  chips: document.querySelectorAll('#assetDockBody #paletteSwatches button').length,
}))()`);
console.log(`  ② 打开后：visible=${opened.visible} 卡的位置=${JSON.stringify(opened.where)} 浮层里色块=${opened.chips}`);
const shotOpen = await capture("asset-dock-open");
if (!opened.visible || !opened.where.every((place) => place === "assetDockBody") || opened.chips <= 0) {
  console.error("❌ 两张卡没有都搬进浮层（或者色块没跟过来）");
  process.exit(1);
}

// ③ **搬过去还能用** ✓：点一个与当前笔刷色不同的色块 ⇒ 笔刷色必须变 ✓
const colourBefore = await evaluate("(document.getElementById('color') || {}).value");
const clicked = await evaluate(`(() => {
  const current = (document.getElementById('color') || {}).value;
  const chip = Array.from(document.querySelectorAll('#assetDockBody #paletteSwatches button'))
    .find((node) => (node.dataset.hex || "").toLowerCase() !== String(current).toLowerCase());
  if (!chip) return null;
  chip.click();
  return chip.dataset.hex;
})()`);
await sleep(400);
const colourAfter = await evaluate("(document.getElementById('color') || {}).value");
console.log(`  ③ 浮层里取色：点 ${clicked} ⇒ 笔刷色 ${colourBefore} → ${colourAfter}`);
if (!clicked || String(colourBefore).toLowerCase() === String(colourAfter).toLowerCase()) {
  console.error("❌ 点浮层里的色块没能改到笔刷色 ⇒ 搬过去之后交互坏了");
  process.exit(1);
}
const shotPick = await capture("asset-dock-pick");

// ④ 收起 ⇒ 卡片回原位、顺序不变 ✓
await evaluate("document.getElementById('assetDockClose').click()");
if (!(await waitFor("!window.yanshiDock.isOpen()", "浮层收起"))) process.exit(1);
const closed = await evaluate(`(() => ({
  where: window.yanshiDock.where(),
  visible: !document.getElementById('assetDock').hidden,
  chips: document.querySelectorAll('#paletteSwatches button').length,
}))()`);
console.log(`  ④ 收起后：visible=${closed.visible} 卡的位置=${JSON.stringify(closed.where)} 色块=${closed.chips}`);
if (closed.visible || JSON.stringify(closed.where) !== JSON.stringify(before.where) || closed.chips !== before.chips) {
  console.error("❌ 卡片没回到原来的位置 / 原来的顺序（或者色块少了）");
  await capture("asset-dock-close-failed");
  process.exit(1);
}
const shotClosed = await capture("asset-dock-closed");

// ⑤ 控制台
const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ⑤ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 6).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      homes: before.homes,
      whereBefore: before.where,
      whereOpened: opened.where,
      colour: { before: colourBefore, after: colourAfter, chip: clicked },
      whereClosed: closed.where,
      screenshots: { open: shotOpen, pick: shotPick, closed: shotClosed },
      consoleErrors: errors.length,
    },
    null,
    2,
  ),
);
ws.close();
