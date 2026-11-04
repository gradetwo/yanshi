#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）

// **★ 总超时守卫 ✗ ★**（2026-10-16 ✓）：**本判据要连 CDP 浏览器** ✓ ⇒ 浏览器起不来时它可能**一直等**
//   （实测：CI 的判据分片被这种判据**拖到跑不完** ✗ ⇒ 我这边十几轮拿不到判定 ✓）。
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✗，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
//   ⇒ **∴ 超时就报「无法判断」并退 3** ✓ —— **不冒充产品失败** ✗、**也不永远挂着** ✗ ★
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 90000);
// 在真实 Chromium 里做「内核 vs 服务端」逐像素自检，按设计 6.1 的 D1 口径判定：
// 差异像素数极少（≤16）且最大通道差 ≤1 LSB 记为通过（判定逻辑在编辑器内的 checkBitExact）。
//
// 前置条件：
//   1) 已运行 yanshi 服务端（默认 127.0.0.1:8110）；
//   2) 已启动带远程调试的 Chromium：
//        chromium --remote-debugging-port=9333 --headless=new about:blank
//
// 用法：
//   node scripts/browser-pixel-check.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..."
//
// 说明：探测的是编辑器自身的自检结果（window.yanshiStats），因此它验证的是**真实用户路径**
// （WASM 内核 + 服务端渲染 + 浏览器合成），不是另写一套比对逻辑。

// **∴ 与 `browser-live-ui.mjs` 同一约定 ✗**（第 65 轮 ✓）：
//   **∴ 编排**给 `browser-*` 的是**API 服务 base**✗
//     ⇒ **∴ 而**本判据**需要一个**真正的查看器**✗（**∴ 那里 `wasm=true` ✓）
//       ⇒ **∴ 于是**：**除 `argv[2]` 外**还认 `YANSHI_LIVE_UI_URL` ✓**** ✓✓
const url = process.env.YANSHI_LIVE_UI_URL || process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
if (!url) {
  console.error("用法: node scripts/browser-pixel-check.mjs <viewer-url>");
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
const target = list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到可用的调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
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

// 等 WASM 内核就绪且拿到服务端 head。
let stats = null;
for (let attempt = 0; attempt < 150; attempt++) {
  stats = await evaluate("window.yanshiStats");
  if (stats?.wasm && stats.kernelHead > 0) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
if (!stats?.wasm) {
  console.error("WASM 内核未就绪（检查服务端 /health 的 wasm 字段与浏览器控制台）");
  process.exit(1);
}
// 触发编辑器内置的位精确自检。
await evaluate('document.querySelector(\'button[data-tool="check"]\').click()');
let out = null;
for (let attempt = 0; attempt < 150; attempt++) {
  out = await evaluate("window.yanshiStats");
  if (out?.diffPixels !== undefined) break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}
if (!out || out.diffPixels === undefined) {
  // **★ 自检**拒绝作答** ＝ **本次没验** ✓，**不是通过 ✗**（第 1520 轮 ✓；主理人裁定 ✓）：
  // 含 heavy 内容（位图／介质笔画 ✓）的文档上，内核**表示不了**它 ✗ ⇒ 编辑器**正确地**拒绝作答 ✓
  // —— **∴ 判据必须如实说「没验」✗**，**而不是** `exit(0)` **冒充「逐位相同」✗**
  //   （**∴ 旧代码在这里 `process.exit(0)` ⇒ **∴ 一次没验的跑动看着像绿 ✗**）。
  console.log("文档 " + (stats?.docId || "(unknown)") + "：**自检不适用**（文档含 heavy 内容 ⇒ 内核表示不了 ⇒ 编辑器拒绝作答 ✓）⇒ **本次未验证**");
  ws.close();
  console.error("⊘ 不可判：编辑器没有给出自检结果（window.yanshiStats.diffPixels 缺失）⇒ 本次没有验证逐位相同（换一个内核能表示的文档，或确认自检按钮真的被点到）");
  process.exit(3);
}
const doc = new URL(url).searchParams.get("doc") ?? "(unknown)";
// **★ `null` ≠ `0` ✗ ★**（第 1520 轮 ✓；主理人裁定 ✓）：
//   **∴ 三档语义**：`true` ⇒ 通过 ✓｜`false` ⇒ 不通过 ✗｜**`null`（内核明确拒绝作答）⇒ 本次没验 ✓**
//   ⇒ **∴ 所以**只有**布尔**才是「作答」✗ ⇒ **∴ 其余一律 `exit(3)`** ✓
//   （**∴ 旧代码 `process.exit(out.bitExact ? 0 : 1)` **把 `null` 归进失败**✗，
//     而上面那条 `!out` 分支又 `exit(0)` **把它当通过**✗ ⇒ **∴ 两个方向都错 ✗**）。
const answered = typeof out.bitExact === "boolean";
const verdict = !answered ? "不适用（未作答）" : out.bitExact ? "通过" : "不通过";
console.log(
  `文档 ${doc}: HEAD ${out.kernelHead}/${out.serverHead} | 差异像素 ${out.diffPixels} | ` +
    `最大通道差 ${out.maxChannelDelta} | 判定 ${verdict}`,
);
ws.close();
if (!answered) {
  console.error(
    "⊘ 不可判：内核拒绝作答（bitExact=" + JSON.stringify(out.bitExact) +
      "）⇒ 本次没有验证逐位相同，既不是通过也不是失败（换一个内核能表示的文档重跑）",
  );
  process.exit(3);
}
process.exit(out.bitExact ? 0 : 1);
