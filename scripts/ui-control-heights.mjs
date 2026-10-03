#!/usr/bin/env node
// **控件高度是否统一**判据（用户原话：布局不规整、没专业软件的设计感）。
//
// 为什么要**真浏览器**量：源码看不出"渲染出来的高度" ✓ —— 两个按钮的 padding/字号/继承稍有差别，
// 屏幕上就是**高度不一** ✗（这正是截图里"收起"比"新建"高的那种问题 ✓）。
// 判据：页面上所有**可见**的 button / input / select 的 `getBoundingClientRect().height`
// 应当落在**少数几档**（≤ 3 档 ✓；业界常见 24/28/32）✓；超出就打印每档的控件与数量 ✓。
//
// 用法：node scripts/ui-control-heights.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
const port = process.argv[3] || "9222";
if (!url) { console.error("用法: node scripts/ui-control-heights.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有可用的页面目标 ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
};
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  socket.send(JSON.stringify({ id, method, params: params || {} }));
});
const evaluate = async (expression) => {
  const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return reply.result?.result?.value;
};
await send("Runtime.enable");
await send("Page.enable");
await send("Page.navigate", { url });
await sleep(2500);
const measured = await evaluate(`(() => {
  const nodes = Array.from(document.querySelectorAll("button, input, select"));
  const out = [];
  for (const node of nodes) {
    const rect = node.getBoundingClientRect();
    if (rect.width < 4 || rect.height < 4) continue;            // 不可见的跳过
    const style = getComputedStyle(node);
    if (style.visibility === "hidden" || style.display === "none") continue;
    out.push({ tag: node.tagName.toLowerCase(), id: node.id || "(无 id)",
               text: (node.textContent || node.value || "").trim().slice(0, 10), h: Math.round(rect.height) });
  }
  return out;
})()`);
if (!measured || !measured.length) { console.error("页面上量不到控件 ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const byHeight = new Map();
for (const item of measured) byHeight.set(item.h, (byHeight.get(item.h) || 0) + 1);
const heights = [...byHeight.keys()].sort((a, b) => a - b);
console.log(`  量到 ${measured.length} 个控件，高度分 ${heights.length} 档：` +
  heights.map((h) => h + "px(" + byHeight.get(h) + ")").join(" "));
if (heights.length > 3) {
  for (const h of heights) {
    const sample = measured.filter((item) => item.h === h).slice(0, 3)
      .map((item) => (item.text || item.id)).join(" / ");
    console.log(`     - ${h}px：${byHeight.get(h)} 个，例如 ${sample}`);
  }
  console.log("  结论：控件高度不统一 ✗（应当收敛到 ≤3 档）");
  process.exit(1);
}
console.log("  ✓ 控件高度统一（≤3 档）");
process.exit(0);
