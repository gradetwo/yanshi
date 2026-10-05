#!/usr/bin/env node
// **离线能导出判据** ✓（(A)⑥ 的第二条 ✓）。
//
// **查证过的实现（✓，不靠记忆 ✗）** ✓：查看器的导出按钮 `#exportPng`（`viewer.rs:534` ✓）
// 在 `viewer.rs:7996` 的处理器里调 **`render_region`** ✓ ⇒ 离线时 `value.ok` 为假 ✗
// ⇒ 只打印"导出失败" ✓ ⇒ **没有任何文件落盘** ✓ ⇒ ⇒ 所以这条判据**天然是红的** ✓。
//
// 判据 ✓：在线开一次页 ✓ ⇒ **断网** ✓ ⇒ **点导出** ✓ ⇒ **下载目录里必须真的出现一个文件** ✓
//（用 CDP 的下载目录 ✓ —— 这比"读一句提示文字"强得多 ✓：**文件是硬证据** ✓）。
const url = process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
const dir = process.argv[4] || "/var/tmp/yanshi-dl";
if (!url) { console.error("用法: node scripts/browser-offline-export.mjs <viewer-url> [cdpPort] [downloadDir]"); process.exit(2); }
const fs = await import("node:fs");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(dir, { recursive: true });
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map(); const logs = [];
socket.onmessage = (event) => { const m = JSON.parse(event.data);
  if (m.method === "Runtime.consoleAPICalled") logs.push(m.params.args?.map((x) => String(x.value ?? x.description ?? "")).join(" ").slice(0, 140));
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
await send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: dir }).catch(() => undefined);
await send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: dir, eventsEnabled: true }).catch(() => undefined);
// ① 在线开一次（把外壳/状态缓存起来 ✓）+ 画一笔 ✓（要有东西可导 ✓）
const parsed = new URL(url);
const doc = parsed.searchParams.get("doc"), token = parsed.searchParams.get("token");
const post = async (tool, args) => (await fetch(`${parsed.origin}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ tool, arguments: args }) })).json();
await post("create_layer", { layer_id: "layer_default", name: "l" });
await post("brush_stroke", { layer_id: "layer_default", brush: "classic-brush", size: 24, color: { r: 10, g: 10, b: 10, a: 255 }, points: [[60, 80, 0.8], [140, 80, 0.8]] });
await send("Page.navigate", { url });
// **等到"页面就绪"这个独立信号** ✓（第 888 轮 ✓）：原来固定睡 5 秒 ✗ ⇒ 机器慢时下面取
// `exportPng` 会取不到 ⇒ 判据间歇红 ✗（**同族问题**：25 个 browser 判据里 20 个都有这种睡眠 ✗）。
// ⚠️ **等待条件必须与断言条件不同** ✗：这里等的是 `readyState` 与按钮存在 ✓，
//    而"断网后能否导出文件"仍由下面的断言判 ✓ ✓。
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { if (await evaluate(`document.readyState === "complete" && !!document.getElementById("exportPng")`)) break; } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
// ② 断网 ✓
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
// ③ 点导出 ✓
const clicked = await evaluate(`(() => { const b = document.getElementById("exportPng"); if (!b) return "no-button"; b.click(); return "clicked"; })()`);
console.log("  导出按钮：" + clicked);
// ⚠️ **这一处**故意保留**有界等待** ✗（第 888 轮 ✓，不要"顺手也改掉" ✗）：
// 点导出后**唯一**可等的信号就是"下载目录里出现文件" ✗ —— 而那正是下面 `:56` 断言的内容 ✗ ⇒
// **一旦"等到文件才继续"，那条断言就永远不会失败** ✗（**第 887 轮我刚犯过这个错** ✓）。
// ⇒ 正确出路是**等一个与断言独立的信号**（CDP 的 `Browser.downloadProgress` 事件 ✓），
//   或像现在这样**留足有界等待** ✓；**"文件是否存在"始终只由断言判** ✓ ✓。
await sleep(5000);
// ④ **硬证据**：下载目录里有没有文件 ✓
const files = fs.readdirSync(dir).filter((name) => !name.endsWith(".crdownload"));
console.log("  下载目录里的文件：" + JSON.stringify(files));
const failures = [];
if (clicked !== "clicked") failures.push("找不到导出按钮 ⇒ 判据无效 ✗");
if (files.length === 0) failures.push("断网后点导出，**没有任何文件落盘** ⇒ 离线导出不可用 ✗");
if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 离线导出：断网时仍然导出了 " + files.join(", "));
socket.close();
process.exit(0);
