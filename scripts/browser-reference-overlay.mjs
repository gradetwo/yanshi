#!/usr/bin/env node
// **参考图叠加的浏览器判据** ✓（需求文档 P2-9 的**可见那一半** ✓；服务端那一半上一轮已交付 ✓）。
// 服务端判据只能证明"**文档没被改**" ✓（那是最要紧的 ✓），但"**看得见一层半透明参考图**"✗
// 必须在**浏览器里**判 ✓ ⇒ 这条走 CDP ✓（与 `browser-undo-disabled.mjs` 同族 ✓）。
//
// 判据 ✓：① 先经 API 设好参考图 ✓（记在偏好里 ✓）；② 打开查看器 ✓；
//        ③ 页面上必须**出现**一个引用那张 blob 的叠加元素 ✓（`img`/`canvas` 均可 ✓）；
//        ④ 它的不透明度必须**接近设置值** ✓（0.5 ⇒ 0.3~0.7 ✓，防"设了没生效" ✗）；
//        ⑤ 经 API 清掉参考图 ⇒ **重新打开后该元素不再出现** ✓。
const url = process.argv[2];
const port = process.argv[3] || process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-reference-overlay.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const hash = "sha256:" + "a".repeat(64);
// ① 先经 API 设参考图（拿页面 URL 里的 doc/token ✓）
const parsed = new URL(url);
const doc = parsed.searchParams.get("doc"), token = parsed.searchParams.get("token");
const post = async (tool, args) => (await fetch(`${parsed.origin}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const set = await post("set_reference", { blob_hash: hash, opacity: 0.5, position: { x: 12, y: 14, w: 120, h: 90 } });
if (set.ok !== true) { console.log("  ✗ set_reference 失败 ⇒ 判据无效 ⇒ " + JSON.stringify(set.context || set.error_code)); process.exit(1); }
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
const inspect = async () => { await send("Page.navigate", { url }); await sleep(3500); return await evaluate(`(() => {
  const short = ${JSON.stringify(hash)};
  const nodes = Array.from(document.querySelectorAll("img, canvas"));
  const hit = nodes.find((node) => String(node.src || node.dataset.blob || "").includes(short) || String(node.dataset.reference || "") === "1" || node.id === "referenceOverlay");
  if (!hit) return { found: false, candidates: nodes.length };
  const style = getComputedStyle(hit);
  return { found: true, tag: hit.tagName, id: hit.id, opacity: Number(style.opacity), zIndex: style.zIndex };
})()`); };
const first = await inspect();
console.log("  设了参考图之后：" + JSON.stringify(first));
const failures = [];
if (!first.found) failures.push("页面上找不到引用该 blob 的叠加元素 ⇒ 参考图看不见 ✗（服务端已记下 ✓，但查看器没画 ✓）");
else if (!(first.opacity >= 0.2 && first.opacity <= 0.8)) failures.push(`叠加层不透明度 ${first.opacity} 与设置的 0.5 不符 ✗`);
// ⑤ 清掉之后必须消失
await post("clear_reference", {});
const second = await inspect();
console.log("  清掉参考图之后：" + JSON.stringify(second));
if (second.found) failures.push("清掉参考图之后页面上仍有叠加元素 ✗");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 参考图：设置后可见且半透明，清除后消失");
