#!/usr/bin/env node
// **不脏读判据** ✓（(A)⑥ 的第三条 ✓："SW 升级后不脏读" ✓）。
//
// **为什么这一条能红** ✓（而不是"顺手也是绿的"✗）：
//   ① 先开页（把状态写进本地缓存 ✓）；
//   ② **在服务端改文档**（经 API 再画一笔 ✓）；
//   ③ **在线重载** ✓ ⇒ 必须**看见新墨** ✓（读了旧值就是脏读 ✗）；
//   ④ **并且**：状态类请求（`get_document`/`list_objects`…）**必须真的发到网络** ✓
//      —— 这条是**变异敏感**的 ✓：谁把可变读改回"本地优先"✗ ⇒ 网络上就不会有这个请求 ⇒ **当场红** ✓
//      （第 171 轮我就是这么把在线画面弄空过一次 ✗ ⇒ 这条判据正是为那次教训立的 ✓）。
const url = process.argv[2];
// **端口一律先从环境变量取** ✓（第 408 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（一个完整 URL）** ✗ ⇒
// 原先 `process.argv[3] || process.env.CDP_PORT` ✗ ⇒ **port 拿到的是 URL** ✗ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **这两个判据一直取不到调试目标** ✗
//（**而它们是"离线"判据** ✓ ⇒ 影响 A⑥ 的验证 ✓）。
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-no-stale-read.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const parsed = new URL(url);
const doc = parsed.searchParams.get("doc"), token = parsed.searchParams.get("token");
const post = async (tool, args) => (await fetch(`${parsed.origin}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const SAMPLER = `(() => {
  const nodes = Array.from(document.querySelectorAll("img, canvas")).filter((n) => { const b = n.getBoundingClientRect(); return b.width >= 64 && b.height >= 64; });
  const board = nodes.find((n) => n.id === "board") || nodes[0];
  if (!board) return { error: "没有画板" };
  const off = document.createElement("canvas");
  off.width = board.width; off.height = board.height;
  const ctx = off.getContext("2d");
  try { ctx.drawImage(board, 0, 0); } catch (error) { return { error: "drawImage 失败" }; }
  const data = ctx.getImageData(0, 0, off.width, off.height).data;
  let dark = 0;
  for (let i = 0; i < data.length; i += 4) if (data[i + 3] > 32 && data[i] < 140 && data[i + 1] < 140 && data[i + 2] < 140) dark += 1;
  return { dark, width: off.width, height: off.height };
})()`;
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map(); const seen = [];
socket.onmessage = (event) => { const m = JSON.parse(event.data);
  if (m.method === "Network.requestWillBeSent") seen.push(m.params.request.url);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
// **等到"页面就绪"这个独立信号** ✓（第 890 轮 ✓）：原来固定睡 4.5 秒 ✗ ⇒ 机器慢时间歇红 ✗。
// ⚠️ **等待条件与断言条件必须不同** ✗：`SAMPLER`（`:26` ✓）读的是**画板像素** ✓，
//    而这里只等 **`readyState` 完成 ＋ `#board` 存在** ✓ ✓（**"画板存在" ≠ "画板像素正确"** ✓）；
//    最后留 **800ms 有界沉降** ✓ 让首帧画完 ✓ —— **"像素对不对"始终只由断言判** ✓ ✓。
const waitReady = async () => {
  for (let i = 0; i < 40; i++) {
    await sleep(300);
    try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("board")')) { await sleep(800); return; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
  }
};
const load = async () => { await send("Page.navigate", { url }); await waitReady(); return await evaluate(SAMPLER); };

await post("create_layer", { layer_id: "layer_default", name: "l" });
await post("brush_stroke", { layer_id: "layer_default", brush: "classic-brush", size: 24, color: { r: 10, g: 10, b: 10, a: 255 }, points: [[50, 70, 0.8], [110, 70, 0.8]] });
const first = await load();
console.log("  第一次（一笔）：" + JSON.stringify(first));
// ② 服务端再画一笔（**文档变了** ✓）
const added = await post("brush_stroke", { layer_id: "layer_default", brush: "classic-brush", size: 24, color: { r: 10, g: 10, b: 10, a: 255 }, points: [[50, 160, 0.8], [110, 160, 0.8]] });
if (added.ok !== true) { console.log("  ✗ 第二次落笔失败 ⇒ 判据无效 ⇒ " + JSON.stringify(added.context || added.error_code)); socket.close(); process.exit(1); }
// ③ 在线重载 ✓（这一轮里数"网络请求"✓）
seen.length = 0;
const second = await load();
console.log("  第二次（两笔）：" + JSON.stringify(second));
const stateRequests = seen.filter((u) => /\/api\/(tools\/)?(get_document|list_objects|list_layers|atoms)/.test(u)).length;
console.log(`  重载期间"状态类"请求数 = ${stateRequests}（**必须 > 0** ✓ —— 走缓存就不会有 ✓）`);
const failures = [];
if (!first || first.error || !second || second.error) failures.push("取样失败 ⇒ 判据无效 ✗");
else {
  if (second.dark <= first.dark) failures.push(`重载后墨量没增加（${second.dark} ≤ ${first.dark}）⇒ **读到旧状态（脏读）** ✗`);
  if (stateRequests === 0) failures.push("重载期间**没有任何状态请求走网络** ⇒ 可变读被写成了本地优先 ⇒ **脏读风险** ✗");
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 不脏读：重载后看到新墨，且状态确实走网络");
socket.close();
process.exit(0);
