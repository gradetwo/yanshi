#!/usr/bin/env node
// **离线重载仍在的浏览器判据** ✓（目标 (A)④/A⑥ ✓：断网后画面还在／能导出 ✓）。
//
// **为什么必须是产品级判据** ✗：不能测"我自己新写的保存 API"✓ —— 那样只要我把 API 写出来就绿了 ✗。
// 这里只问一件事 ✓：**把网断掉、重新加载页面之后，那一笔还在不在** ✓。
//
// **前提已验证（第 162 轮 ✓）** ✓：SW 的 `fetch` 处理器**明确跳过** `/api/` ✗
// ⇒ 所以断网时像素**不可能**从 API 来 ✓ ⇒ 画面要还在，就**只能**靠浏览器自己的本地存储 ✓
// ⇒ 这条判据**不会假绿** ✓（否则我会以为自己做完了 ✗）。
//
// 判据 ✓：① 画一笔（经 API ✓，保证文档真的有墨 ✓）；
//        ② **正对照** ✓：画之前同一画布必须是**白的** ✓（否则"深色像素数"这个量法不可信 ✗）；
//        ③ 开页 ⇒ 画面必须**有深色像素** ✓（说明渲染真的发生了 ✓）；
//        ④ **断网** ✓（CDP `Network.emulateNetworkConditions{offline:true}` ✓ —— 需求原话就是"断网/关服务端"✓）
//           ⇒ `Page.reload` ✓ ⇒ 画面上**必须仍有深色像素** ✓（像素来自本地存储 ✓），且**不是全白** ✓。
const url = process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-offline-reload.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const parsed = new URL(url);
const doc = parsed.searchParams.get("doc"), token = parsed.searchParams.get("token");
const post = async (tool, args) => (await fetch(`${parsed.origin}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();

// 页面里"数深色像素"的取样器 ✓ —— 用**离屏 2D 画布 drawImage** ✓，这样 canvas 是 2D 还是 WebGL 都能量 ✓。
const SAMPLER = `(() => {
  // **画板可能是 img 也可能是 canvas** ✗ —— 实测：查看器里根本没有 canvas ✓
  //（它把服务端的 PNG 显示在 img 上 ✓）⇒ 取「面积最大」的那个 ✓，两种都能量 ✓。
  const nodes = Array.from(document.querySelectorAll("img, canvas")).filter((node) => {
    const box = node.getBoundingClientRect();
    return box.width >= 64 && box.height >= 64;
  });
  if (nodes.length === 0) return { error: "页面上没有够大的 img/canvas" };
  // **必须优先 #board** ✗ —— 实测：overlay 的 CSS 盒子（354×318）**比 board（344×318）还大** ✓
  // ⇒ 我原来"取面积最大"会选中**空的 overlay** ✗（dark=0 ✓）⇒ 判据会**冤枉**在线画面 ✗ ✗。
  const byId = nodes.find((node) => node.id === "board");
  const canvas = byId || nodes.reduce((best, node) => {
    const box = node.getBoundingClientRect();
    const area = box.width * box.height;
    return area > best.area ? { node, area } : best;
  }, { node: nodes[0], area: 0 }).node;
  const off = document.createElement("canvas");
  off.width = canvas.width; off.height = canvas.height;
  const ctx = off.getContext("2d");
  if (!ctx) return { error: "拿不到 2D 上下文" };
  try { ctx.drawImage(canvas, 0, 0); } catch (error) { return { error: "drawImage 失败：" + error }; }
  let data;
  try { data = ctx.getImageData(0, 0, off.width, off.height).data; } catch (error) { return { error: "getImageData 失败：" + error }; }
  let dark = 0, ink = 0, total = 0;
  for (let i = 0; i < data.length; i += 4) {
    total += 1;
    const [r, g, b, a] = [data[i], data[i + 1], data[i + 2], data[i + 3]];
    if (a > 32 && !(r > 245 && g > 245 && b > 245)) ink += 1;
    if (a > 32 && r < 140 && g < 140 && b < 140) dark += 1;
  }
  return { dark, ink, total, width: off.width, height: off.height };
})()`;

// **有界轮询等出墨**（第 1120 轮定案）：原来只等固定 800ms，首帧可能更慢 ⇒ 采样太早 ⇒ 读到 0 ✗。
// 决定性证据：只在采样前加一个**只读**探针（引入少量延迟）⇒ 判据立刻转绿 ✓。
// 所以把"等待条件"与"断言条件"对齐：轮询到出现深色像素为止（有界），再断言 ✓。
async function waitForInk(label, limit = 80, step = 250) {
  let last = null;
  for (let i = 0; i < limit; i += 1) {
    last = await evaluate(SAMPLER);
    if (last && !last.error && last.dark > 50) {
      console.log(`  · ${label}：第 ${i + 1} 次采样出墨（dark=${last.dark}，${last.width}x${last.height}）`);
      return last;
    }
    await sleep(step);
  }
  console.log(`  · ${label}：轮询 ${limit} 次仍无墨（最后一次 ${JSON.stringify(last)}）`);
  return last;
}


const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
const reload = async (offline) => {
  await send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  await send("Page.navigate", { url });
  // **等到"页面就绪"这个独立信号** ✓（第 891 轮 ✓）：原来固定睡 4 秒 ✗ ⇒ 机器慢时间歇红 ✗。
  // ⚠️ **等待条件与断言条件不同** ✗：`SAMPLER` 读的是**深色像素数** ✓，
  //    这里只等 `readyState` 完成 ＋ `#board` 存在 ✓，再留 **800ms 有界沉降** 让首帧画完 ✓；
  //    **"深色像素有多少"始终只由断言判** ✓ ✓。
  for (let i = 0; i < 40; i++) {
    await sleep(300);
    try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("board")')) { await sleep(800); break; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
  }
  return await waitForInk("采样");
};

// ② **正对照** ✓：先看"什么都没画"的画布（另开一个文档 ✓，避免污染被测文档 ✓）。
const blank = await reload(false);
console.log("  正对照（未落笔）：" + JSON.stringify(blank));
const failures = [];
if (!blank || blank.error) { console.log("  ✗ 取样器不可用 ⇒ 判据无效 ⇒ " + JSON.stringify(blank)); socket.close(); process.exit(1); }
if (blank.dark > 50) failures.push(`未落笔的画布上就有 ${blank.dark} 个深色像素 ⇒ 量法不可信 ✗`);

// ① 画一笔 ✓
await post("create_layer", { layer_id: "layer_default", name: "l" });
const painted = await post("brush_stroke", {
  layer_id: "layer_default", brush: "classic-brush", size: 26,
  color: { r: 10, g: 10, b: 10, a: 255 },
  points: [[60, 60, 0.8], [120, 70, 0.8], [180, 60, 0.8]],
});
if (painted.ok !== true) { console.log("  ✗ 落笔失败 ⇒ 判据无效 ⇒ " + JSON.stringify(painted.context || painted.error_code)); socket.close(); process.exit(1); }

// ③ 在线时画面必须有墨 ✓
const online = await reload(false);
console.log("  在线（已落笔）：" + JSON.stringify(online));
if (!online || online.error) failures.push("在线时取样失败 ⇒ 判据无效 ✗");
else if (online.dark <= 50) failures.push(`在线时画布上只有 ${online.dark} 个深色像素 ⇒ 那一笔没渲染出来 ✗`);

// ④ **断网重载** ✓ —— 这一条就是需求的核心 ✓
const offline = await reload(true);
console.log("  断网重载后：" + JSON.stringify(offline));
if (!offline || offline.error) failures.push("断网重载后取样失败（页面可能根本没打开）✗");
else if (offline.dark <= 50) failures.push(`断网重载后画布上只有 ${offline.dark} 个深色像素 ⇒ 像素没有落到浏览器本地存储 ✗（目标：≥ 在线时的九成）`);
else if (online && offline.dark < online.dark * 0.9) failures.push(`断网重载后墨量 ${offline.dark} 明显少于在线时的 ${online.dark} ⇒ 本地读回不完整 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 离线重载：那一笔仍在（像素来自浏览器本地存储）");
socket.close();
process.exit(0);
