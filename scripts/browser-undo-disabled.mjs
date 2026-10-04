#!/usr/bin/env node
// **「撤销在空栈上必须不可点」判据** ✓（(B)② 里定位到的真缺陷 ✓）。
// 背景 ✓：`undo.disabled = undoCount === 0` ✗ —— 而**计数未知**时 `undoCount` 是 `null` ✓
// ⇒ `null === 0` 为**假** ✗ ⇒ 按钮**保持可点** ✓ ⇒ 用户点了没反应 ✓（判据实测：`撤销/重做栈深度：null`）。
// 用法：node scripts/browser-undo-disabled.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-undo-disabled.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable");
await send("Page.navigate", { url });
await sleep(3000);
// **等"计数未知"这个状态真正出现** ✓：它正是老代码会误判的那个状态 ✓。
const state = await evaluate(`(() => {
  const undo = document.querySelector('button[data-tool="undo"]');
  const redo = document.querySelector('button[data-tool="redo"]');
  return {
    undoDisabled: undo ? undo.disabled : null,
    redoDisabled: redo ? redo.disabled : null,
    stackText: (document.getElementById("last") || {}).textContent || "",
  };
})()`);
console.log("  最新响应里的栈深度：" + JSON.stringify(String(state.stackText).slice(0, 80)));
console.log("  undo.disabled=" + state.undoDisabled + "｜redo.disabled=" + state.redoDisabled);
const failures = [];
if (state.undoDisabled === null || state.redoDisabled === null) failures.push("页面上没有撤销/重做按钮 ⇒ 判据无效");
// ① 空栈（或计数未知）⇒ **必须不可点** ✓（这正是老代码错的地方 ✗）
if (state.undoDisabled !== true) failures.push("空栈/计数未知时「撤销」仍可点（应当 disabled）");
if (state.redoDisabled !== true) failures.push("空栈/计数未知时「重做」仍可点（应当 disabled）");
// ② 覆盖检查 ✓（**问对元素** ✗ —— 我上一版去读 `#last` ✓，那里装的是**工具响应 JSON** ✗、
// 与栈深度无关 ✓ ⇒ 于是误报"覆盖不到" ✗）。这里直接读**按钮状态所依据的那个来源** ✓：
// 新文档上"可撤销"必须是 **0 或未知** ✓（即**没有可撤销的东西** ✓）⇒ 那样才对得上第 ① 条断言 ✓。
const counts = await evaluate(`(() => {
  const text = document.body ? document.body.innerText : "";
  const match = text.match(/可撤销\s*(\S+)\s*笔/);
  return match ? match[1] : null;
})()`);
console.log("  页面自称的可撤销数：" + JSON.stringify(counts));
if (counts !== null && counts !== "0" && counts !== "—") {
  failures.push("新文档上却自称可撤销 " + counts + " 笔 ⇒ 覆盖不到目标状态");
}
socket.close();
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 空栈/计数未知时，撤销与重做都不可点");
process.exit(0);
