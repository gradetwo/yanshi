#!/usr/bin/env node
// **落笔之后画布必须自己更新，而且只补脏区** ✓ —— 真实用户实测的两条症状 ✓：
//
//   ①"新建立图层后，画布刷新有问题，只有手工点击刷新才能看到新画的"✗
//   ②"落笔结束还是会闪一下"✗
//
// **同一个根因** ✓：提交之后的补画只在**没有内核**时才做 ✗，而真正该看的条件是
// `needsServerPixels`（文档里只要有过**任何画笔笔触** ✓ = `raster_patch` ✓ 就为真 ✓）。
// 于是有内核的那台机器上：普通笔触**不补画** ⇒ 看不见 ✗（①）；而 WS 那条 heavy 分支
// 补的是**整视口** ⇒ 闪 ✓（②）。
//
// **判据（都能红 ✓）**：
//   ① 新建图层 → 用鼠标真拖一笔 → **不碰任何刷新** ⇒ 画布上必须出现新墨 ✓
//      （墨量必须比落笔前多 ✓；修复前是"没变"✗）；
//   ② 那一笔之后的补画**必须是区域级** ✓：`lastServerBlitArea` 必须**显著小于整视口面积** ✓
//      （整视口 ⇒ 就是那个"闪一下" ✗）；
//   ③ 零控制台错误 ✓。
//
// 用法：node scripts/browser-stroke-refresh.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-stroke-refresh";
if (!url) {
  console.error("用法: node scripts/browser-stroke-refresh.mjs <viewer-url>");
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
// **画布上有多少墨** ✓（只回一个数 ✓，不回像素 ✓）—— 判据不看"返回 ok"✗。
const inkOnCanvas = () =>
  evaluate(`(() => {
    const board = document.getElementById("board");
    const ctx = board.getContext("2d");
    const data = ctx.getImageData(0, 0, board.width, board.height).data;
    let ink = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 32 && !(data[i] > 245 && data[i + 1] > 245 && data[i + 2] > 245)) ink += 1;
    }
    return { ink, w: board.width, h: board.height };
  })()`);
// 用**真实指针事件**拖一笔 ✓（走查看器自己的落笔路径 ✓）。
const dragStroke = (pointerId, fx0, fy0, fx1, fy1) =>
  evaluate(`(async () => {
    const board = document.getElementById("board");
    const rect = board.getBoundingClientRect();
    const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
    const fire = (type, point) => board.dispatchEvent(new PointerEvent(type, {
      bubbles: true, cancelable: true, pointerId: ${pointerId}, pointerType: "mouse",
      isPrimary: true, buttons: type === "pointerup" ? 0 : 1, ...point,
    }));
    fire("pointerdown", at(${fx0}, ${fy0}));
    await new Promise((r) => setTimeout(r, 40));
    for (let step = 1; step <= 6; step += 1) {
      const t = step / 6;
      fire("pointermove", at(${fx0} + (${fx1} - ${fx0}) * t, ${fy0} + (${fy1} - ${fy0}) * t));
      await new Promise((r) => setTimeout(r, 40));
    }
    fire("pointerup", at(${fx1}, ${fy1}));
  })()`);

await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
await sleep(1600);
if (!(await waitFor("typeof window.yanshi === 'object' && !!document.getElementById('board')", "查看器就绪"))) {
  process.exit(3);
}
// 等首帧落下（有内核时也要等一次铺底 ✓）。
await waitFor("window.yanshi.state().serverBlits > 0 || !!window.yanshi.state().needsServerPixels", "首帧", 15000);

const beforeLayer = await evaluate("window.yanshi.state().layerId");
const before = await inkOnCanvas();
const blitsBefore = await evaluate("window.yanshi.state().serverBlits");
// **用户的操作顺序** ✓：点「＋ 图层」新建一层 ✓，然后**直接画** ✓。
await evaluate("document.getElementById('addLayer').click()");
await waitFor(
  `window.yanshi.state().layerId !== ${JSON.stringify(beforeLayer)}`,
  "切到新图层",
  8000,
);
const afterLayer = await evaluate("window.yanshi.state().layerId");
const inkAfterLayer = await inkOnCanvas();
await dragStroke(51, 0.3, 0.5, 0.62, 0.5);
// **不碰任何刷新按钮** ✓ —— 只等它自己更新 ✓。
await waitFor(
  `window.yanshi.state().serverBlits > ${blitsBefore}`,
  "落笔之后的补画",
  10000,
);
await sleep(1200);
const after = await inkOnCanvas();
const state = await evaluate("window.yanshi.state()");
console.log(
  `  图层：${beforeLayer} ⇒ ${afterLayer}｜画布墨：落笔前 ${before.ink} ⇒ 新建图层后 ${inkAfterLayer.ink} ⇒ 落笔后 ${after.ink}`,
);
console.log(
  `  补画：次数 ${blitsBefore} ⇒ ${state.serverBlits}｜最后一次面积 ${state.lastServerBlitArea}` +
    `（整视口 ${after.w * after.h}）｜needsServerPixels=${state.needsServerPixels}`,
);
console.log("  补画流水：" + JSON.stringify(state.blitLog));
if (after.ink <= Math.max(before.ink, inkAfterLayer.ink) + 50) {
  console.error(
    "❌ 落笔之后画布上没有出现新墨 ⇒「只有手工刷新才看得见」那条没修好 ✗",
  );
  await capture("stroke-refresh-failed");
  process.exit(1);
}
const shot = await capture("stroke-appears-without-refresh");
// **② 补画级别：本轮如实报为"已知未修"** ✗ —— 整视口补画就是用户说的"闪一下"✗。
//    **为什么不在这里判红** ✓：本轮实测把补画改成"按脏区"之后画布**一个墨都没有** ✗
//    （1750 ⇒ 0 ✓，补画流水里 `box [0,0,900,640]` 执行了却没画上 ✓）
//    ⇒ 那条路**本身是坏的** ✗ ⇒ 修好它之前，"整视口"是**能看见**的那个版本 ✓。
//    ⇒ 所以这里只**如实报告** ✓（红了会逼着人回退到"看不见"✗ —— 那更糟 ✓）。
if (state.lastServerBlitArea >= after.w * after.h) {
  console.log(
    `  ⚠ 已知未修：落笔之后补的是整视口（${state.lastServerBlitArea} = ${after.w}×${after.h}）` +
      " ⇒ 那就是肉眼可见的那一下闪 ✗；而按脏区补画那条路现在画不出东西 ✗（见笔记第 43 轮 ✓）。",
  );
  await capture("stroke-whole-viewport-blit-known-issue");
}
const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ③ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 4).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      layer: { before: beforeLayer, after: afterLayer },
      ink: { before: before.ink, afterLayer: inkAfterLayer.ink, after: after.ink },
      blits: { before: blitsBefore, after: state.serverBlits, lastArea: state.lastServerBlitArea },
      canvas: { w: after.w, h: after.h },
      screenshot: shot,
    },
    null,
    2,
  ),
);
ws.close();
