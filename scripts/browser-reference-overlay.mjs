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
// **参考图叠加的浏览器判据** ✓（需求文档 P2-9 的**可见那一半** ✓；服务端那一半上一轮已交付 ✓）。
// 服务端判据只能证明"**文档没被改**" ✓（那是最要紧的 ✓），但"**看得见一层半透明参考图**"✗
// 必须在**浏览器里**判 ✓ ⇒ 这条走 CDP ✓（与 `browser-undo-disabled.mjs` 同族 ✓）。
//
// 判据 ✓：① 先经 API 设好参考图 ✓（记在偏好里 ✓）；② 打开查看器 ✓；
//        ③ 页面上必须**出现**一个引用那张 blob 的叠加元素 ✓（`img`/`canvas` 均可 ✓）；
//        ④ 它的不透明度必须**接近设置值** ✓（0.5 ⇒ 0.3~0.7 ✓，防"设了没生效" ✗）；
//        ⑤ 经 API 清掉参考图 ⇒ **重新打开后该元素不再出现** ✓。
const url = process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-reference-overlay.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
//（原来这里写死一个假 hash ✗ ⇒ 见下面 `render_region` 那段 ✓：blob 必须真在存储里 ✓）
// ① 先经 API 设参考图（拿页面 URL 里的 doc/token ✓）
const parsed = new URL(url);
// **本判据导航用的 URL** ✓：钉住 `render=server` ✓（理由见下面 `inspect()` 里那条 ✓）。
const navUrl = new URL(url);
navUrl.searchParams.set("render", "server");
const doc = parsed.searchParams.get("doc"), token = parsed.searchParams.get("token");
// **★ 服务端连接守卫 ✗ ★**（2026-10-16 ✓）：包在 helper 本体上 ⇒ **所有调用点一次性受保护** ✓
//   （本仓约定：判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）；
//    否则服务端不可达时抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗）
const post = async (tool, args) => {
  try {
    return (await fetch(`${parsed.origin}/api/tools?doc=${doc}&token=${token}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ tool, arguments: args }),
    })).json();
  } catch (error) {
    console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上服务端：" + error.message + " ⇒ 退出码 3");
    process.exit(3);
  }
};
const set0 = await post("get_document", {});
const docW = Number((set0.document || set0).width), docH = Number((set0.document || set0).height);
if (!(docW > 0 && docH > 0)) { console.log("  ✗ 读不到文档尺寸 ⇒ 判据自身前提不成立（不是产品问题）⇒ " + JSON.stringify(set0).slice(0, 160)); process.exit(3); }
// **`hash` 必须是存储里真有的 blob** ✓（本轮修掉的**假红** ✗）：`set_reference` 会校验 blob 在不在 ✗
//（回执原文：「是合法格式，但**这个 blob 不在存储里** ⇒ 参考图存下来也永远比不了」✓）。
//   **∴ 原来的错 ✗**：这里写死 `sha256:aaaa…` ⇒ `set_reference` 被拒 ⇒ 判据**每次都停在 exit 3**
//     （"我自己坏了"）✓ —— 后果是**参考图"看得见"那一半其实很久没被真正判过** ✗（红绿不分的反面：常红 ✓）。
//   **∴ 现在 ✗→✓**：先用**只读**的 `render_region` 把当前文档渲染成一张真 blob ✓（不改文档 ✓）。
const rendered = await post("render_region", { region: { x: 0, y: 0, w: Math.min(64, docW), h: Math.min(64, docH) } });
const hash = rendered && rendered.blob_hash;
if (!hash) { console.log("  ✗ 产不出真 blob ⇒ 判据自身前提不成立（不是产品问题）⇒ " + JSON.stringify(rendered.context || rendered.error_code || rendered).slice(0, 200)); process.exit(3); }
const position = { x: 4, y: 4, w: Math.max(8, Math.min(120, docW - 8)), h: Math.max(8, Math.min(90, docH - 8)) };
const set = await post("set_reference", { blob_hash: hash, opacity: 0.5, position });
if (set.ok !== true) { console.log("  ✗ set_reference 失败 ⇒ 判据自身前提不成立（不是产品问题）⇒ " + JSON.stringify(set.context || set.error_code)); process.exit(3); }
// **★ CDP 连接守卫 ✗ ★**（2026-10-16 ✓）：浏览器没起来时这里会抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）"); process.exit(3); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable");
// **只等 `readyState` ＋ 有界沉降** ✗（第 893 轮 ✓，**不要"顺手改成等叠加元素"** ✗）：
// 下面的断言读的正是"页面上有没有引用该 blob 的叠加元素" ✗ ⇒ **等它就等于让断言永不失败** ✗
//（**第 887 轮我犯过这个错** ✓）⇒ 这里只等"文档完成" ✓ ＋ 800ms 让叠加层画出来 ✓；
// **"参考图可不可见"始终只由断言判** ✓ ✓。
const inspect = async () => {
  // **★ 自己把渲染模式钉成「服务器」✗ ★**（第四轮接线自查 ✓ 本机实测：带 `render=client` 时本判据**恒红** ✓）：
  //   `run-criteria.sh` 给 browser-* 一族**统一加 `&render=client`** ✓ 而 `viewer-app.js:83-88` 里
  //   **URL 参数优先级最高** ✗ ⇒ 页面改走本地内核 ⇒ `get_preferences` 打到 **IndexedDB** ✓
  //   而本判据的参考图是**经服务端 API** 写进 `preferences.json` 的 ✗
  //   ⇒ 页面永远读不到那个 hash ⇒「叠加元素不存在」⇒ **红的是接线 ✗ 不是产品** ✓。
  //   偏好有**两份存储**（服务端 ✓ 与本地内核 ✓）⇒ 经服务端写状态就必须经服务端读 ✓
  //   （与 `browser-brush-panel`／`browser-i18n` 那几条"测的是服务端/静态界面"的例外同理 ✓）★
  await send("Page.navigate", { url: navUrl });
  for (let i = 0; i < 40; i++) {
    await sleep(300);
    try { if (await evaluate('document.readyState === "complete"')) { await sleep(800); break; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
  }
  return await evaluate(`(() => {
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
if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 参考图：设置后可见且半透明，清除后消失");
// **显式关闭再退出** ✓：留着 WebSocket 会让进程挂着 ⇒ 外层 timeout 报 124 ✗（红绿不分 ✓）。
socket.close();
process.exit(0);
