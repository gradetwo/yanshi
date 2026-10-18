#!/usr/bin/env node
// **「适配」必须把整幅画放进**真实可见的画布宿主**✗**（外部复测 QY-D04 的判据 ✓）
//
// **∴ 缺陷 ✓**：**∴ `availableArea` 的高度原本是**`window.innerHeight - 96`**✗**（**∴ 96 是猜的 ✓**）
//   ⇒ **∴ 而**工具栏换行／面板／状态栏出现时**✗ ⇒ **∴ 真实可用高度**更小**✗**
//   ⇒ **∴ 于是**适配把画面缩进一个**溢出的容器**✗ ⇒ **∴ 底部色条**看不见**✗**
//     （**∴ 他们实测 ✓**：**∴ 约 13% ✗、顶见底不见 ✓；全屏模式同 13% 两个都见 ✓**）
//
// **∴ 判据 ✓**：**∴ 点「适配」之后 ✗ ⇒ **∴ 画布元素（`main canvas`）的 rect**✗
//   ⇒ **∴ 必须**完全落在**它的宿主容器 rect 内**✗**（**∴ 容差 2px ✓**）
//   ⇒ **∴ 且**宿主的可见高度**必须**被用满到 90% 以上**✗**（**∴ 防止把画面缩得过小 ✓**）
//
// 用法：node scripts/browser-fit-fits-visible-host.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
const port = process.argv[3] || "9222";
if (!url) { console.error("用法: node scripts/browser-fit-fits-visible-host.mjs <viewer-url> [cdpPort]"); process.exit(2); }

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
let targets;
try {
  targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
} catch (error) {
  console.error(`❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP（127.0.0.1:${port}）：${error.message} ⇒ 退出码 3`);
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有可用的页面目标 ⇒ 判据无法运行（不是通过）⇒ 退出码 3"); process.exit(3); }

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((done, fail) => { socket.onopen = done; socket.onerror = fail; });
let nextId = 0;
const waiting = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && waiting.has(message.id)) { waiting.get(message.id)(message); waiting.delete(message.id); }
};
const send = (method, params = {}) =>
  new Promise((done) => { const id = ++nextId; waiting.set(id, done); socket.send(JSON.stringify({ id, method, params })); });

// **∴ 两个开关必须成对 ✗**（**∴ AGENTS.md 第 402 轮 ✓**：缺 `awaitPromise` 会**静默返回 {}** ✓）
const evaluate = async (expression) => {
  const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  return reply.result?.result?.value;
};

await send("Page.enable");
await send("Runtime.enable");
await send("Page.navigate", { url });
await sleep(4000);

const problems = [];
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) problems.push(why); };

// **∴ 载入一幅**上下都有标记**✗ 的文档 ✓** —— 用 API 画两条横贯全宽的色带 ✓
const setup = await evaluate(`(async () => {
  const tok = new URL(location.href).searchParams.get("token") || "";
  const doc = (await (await fetch("/api/documents", {method:"POST",headers:{"content-type":"application/json"},
    body: JSON.stringify({ doc_id: "fitprobe_" + Date.now(), width: 1200, height: 900 })})).json());
  const d = new URL(doc.url || location.href).searchParams.get("doc") || "";
  const call = (n, a) => fetch("/api/tools/" + n + "?doc=" + encodeURIComponent(d) + "&token=" + encodeURIComponent(doc.token || tok),
    { method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify(a) }).then(r => r.json());
  await call("create_layer", { layer_id: "L", name: "L" });
  // 顶部红带 + 底部蓝带（各占 8% 高，横贯全宽）
  for (const [y, color] of [[40, {r:220,g:40,b:40,a:255}], [860, {r:40,g:80,b:220,a:255}]]) {
    await call("fill_region", { layer_id: "L", rect: [0, y-40, 1200, 80], color });
  }
  return { doc: d, token: doc.token || tok };
})()`);
if (!setup?.token) {
  console.log("  ❌ 判据自身前提不成立（建文档失败）⇒ 退出码 3");
  process.exit(3);
}
await send("Page.navigate", { url: `${url}${url.includes("?") ? "&" : "?"}doc=${encodeURIComponent(setup.doc)}&token=${encodeURIComponent(setup.token)}` });
await sleep(4000);

// **∴ 点「适配」✗**（**∴ 找那个按钮 ✓**）
const clicked = await evaluate(`(() => {
  const el = [...document.querySelectorAll("button")].find(b => /适配|fit/i.test(b.textContent || "") || b.id === "fit");
  if (!el) return "no-button";
  el.click();
  return "clicked";
})()`);
if (clicked !== "clicked") { console.log(`  ❌ 判据自身前提不成立（找不到「适配」按钮：${clicked}）⇒ 退出码 3`); process.exit(3); }
await sleep(1200);

const measured = await evaluate(`(() => {
  const canvas = document.querySelector("main canvas");
  if (!canvas) return null;
  const host = canvas.parentElement;
  const c = canvas.getBoundingClientRect(), h = host.getBoundingClientRect();
  return { c: { x: c.x, y: c.y, w: c.width, h: c.height },
           h: { x: h.x, y: h.y, w: h.width, h: h.height },
           tolerance: 2 };
})()`);
if (!measured) { console.log("  ❌ 判据自身前提不成立（找不到 main canvas）⇒ 退出码 3"); process.exit(3); }

const { c, h } = measured;
console.log(`  画布 rect：y=${c.y.toFixed(1)} h=${c.h.toFixed(1)}（底 ${(c.y + c.h).toFixed(1)}）`);
console.log(`  宿主 rect：y=${h.y.toFixed(1)} h=${h.h.toFixed(1)}（底 ${(h.y + h.h).toFixed(1)}）`);
check(c.y >= h.y - 2, `画布顶端不高于宿主顶端（${c.y.toFixed(1)} vs ${h.y.toFixed(1)}）`);
check(c.y + c.h <= h.y + h.h + 2, `画布底端不超出宿主底端（${(c.y + c.h).toFixed(1)} vs ${(h.y + h.h).toFixed(1)}）`);
check(h.h > 240 ? c.h >= h.h * 0.9 : true, `画面高度用满宿主 90% 以上（${c.h.toFixed(1)} vs ${h.h.toFixed(1)}）`);

socket.close();
if (problems.length) { console.log(`  ✗ 适配没有落在真实可见宿主内（${problems.length} 处）`); process.exit(1); }
console.log("  ✓ 适配把整幅画放进了真实可见的画布宿主内");
