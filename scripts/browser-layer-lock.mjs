#!/usr/bin/env node
// **判据：往「锁定的图层」落笔时，界面必须当面谈清楚**（第六轮新增）。
//
// **抓的是哪一类缺陷**（本轮实测出来的 ✗ 不是读代码猜的 ✓）：
//   服务端**早就**拒得住 —— 锁定层上 `draw_stroke` / `erase` / `draw_shape` / `draw_text` /
//   `import_image`（介质）/ `fill_region` / `clone_stamp` / `heal_stamp` / `smudge` / `liquify_*` /
//   `move_object` / `transform_object` / `delete_object` 全部 **HTTP 403 `permission_denied`** ✓；
//   可 Web 这边**一笔照画** ⇒ 请求出门 ⇒ 被拒 ⇒ 「失败」两个字只写进 `#log` ✓ 而 `#log` 在
//   `data-panel="history"` 那张**默认收起**的卡片里（实测 `logVisible:false`、`scrollHeight:0` ✗）
//   ⇒ 用户得到的全部信息就是「按了没反应」✗（更糟：`render=client` 下本地乐观像素回滚不干净 ✓
//   会在画布上留下一笔服务端根本没有的**幻影墨** ✗ 见本轮说明的第 3 节 ✓）。
//
// **钉住的十二件事**（每条都断言**服务端事实 + 屏幕事实** ✗ 不是「日志里有这行字」✓）：
//   ① 前提：服务端对锁定层真的会拒（`draw_stroke` ⇒ 403 `permission_denied`）✗ 否则本判据测的不是那条路 ⇒ `exit(3)`；
//   ② 在图层面板**点 🔒**（走 `update_layer` ✓ 不是 HTTP 后门 ✓）之后 ⇒ `list_layers` 说 `locked:true` ✓；
//   ③ 锁定层上画笔拖动 ⇒ **原子数一格都不多** ✓ **对象数不变** ✓ **画布像素与拖之前逐块相同** ✓
//      ✓ **而且一个写入请求都没发出**（`/api/tools/*` 计数器 ✓ 只盯服务端状态的话 ⇒ 抽掉预检照样绿 ✓
//      这条盲区是**本轮变异测试实测出来的** ✗）；
//   ④ 同一刻浮层 `#stageNotice` **看得见**（`hidden:false` ✓ 有尺寸 ✓ 在视口内 ✓）而且说的就是那句锁定解释 ✓
//      —— 这才是本判据的核心：消息必须到**用户一定看得见**的那一处 ✗ 不是收起来的日志 ✓；
//   ⑤ 文本工具点一下同样被拦（另一条分支 ✓ 不走通用落笔那条 ✓）⇒ 原子数仍不变 ✓ 浮层仍在说 ✓
//      ✓ **`window.prompt` 一次都没被调**（拦要在弹窗**之前** ✓ 弹一个注定没用的框同样是缺陷 ✓）；
//   ⑥ **不许误伤合法的活**：同一层锁着 ⇒「矩形蒙版」拖一把**照旧成功** ✓（实测 `create_mask` 对锁定层
//      回 200 ✓ `paint_mask` 也放行 ✓）⇒ 原子数必须 +1 ✓ 且浮层**没有**说锁定那句 ✗；
//   ⑦ 解锁之后同一把拖动 ⇒ 落笔成功 ✓ 原子 +1 ✓ 画布上真的出现墨 ✓（非白像素数上升 ✓）
//      ⇒ 证明 ③ 的「没画上去」是拦得住 ✓ 不是「这个工具根本画不上」那种假绿 ✗；
//   ⑧ 英文模式下浮层那句**句式框架**必须是英文（0 个汉字 ✓）⇒ 新句子漏词条就是这里红 ✗；
//      **图层名是服务端数据** ✓ 不在判定之内 ✗（与 `browser-fill-region` 同一条口径 ✓）；
//   ⑨ `render=client` 下同样的锁定拖动 ⇒ 本地内核**一个原子都不许多** ✓ 画布不许留下幻影像素 ✓
//      （内核起不来 ⇒ 判据自己失效 ⇒ `exit(3)` ✗ 不冒充产品红 ✓；**内核里必须先有历史** ✓
//      否则「0 ⇒ 0 没多」是**空判** ✗ run2 实测到这种假绿 ⇒ 加了有界等待 + 前提 `exit(3)` ✓）；
//   ⑩ 全程 0 条页面 JS 错误 ✓（预检新加的那几条不许把落笔路径打断 ✗）；
//   ⑪ **锁是另一头上的**（同事／MCP 直接 `update_layer` ✗ 界面从没点过那颗 🔒）⇒ 预检的缓存
//      一开始**不知道** ✗ 这一笔必然发出请求并被 403 兜住 ⇒ 断言三件事：
//      原子与画布**一分不变**（不许幻影）✓ 用户**听到解释** ✓ 且被拒之后界面**自己认账**
//      （那一行翻成 🔒 ✓ 第二笔就得**在出门前**拦住 ✓）—— 这条是 ⑧ 那次飘出来的 ✓
//      量出来「界面永远停在 🔓」是真缺陷 ⇒ 本轮已在 `callToolChecked` 里补了状态回读 ✓；
//   ⑫ **本地内核装不下这份文档**（注入一份内核必定解析不了的原子 ✓ 与实测到的间歇故障同一条形状 ✗）
//      ⇒ 界面不许留一块白布 ✗ 不许把 `kernelHead` 推到服务端的头部（谎报同步 ✓）
//      ⇒ 必须「当面谈 + 退回服务端渲染 + 记一笔账」✓ 且画布上**真的有画面** ✓ 中英两边都要说对 ✓。
//
// **等前提的口径**（本轮 ⑧ 飘一次之后改的 ✓）：凡「点了锁」之后 ✗ 必须等**页面自己**认账
//   （那一行 glyph 翻成 🔒／🔓 ✗ 与界面语言无关 ✓ 且与预检缓存出自**同一次渲染** ✓）
//   ＋ 服务端也说一样 ✗ 只 poll 服务端就落笔 ⇒ 测到的是「后端兜底」那条 ✗ 不是「预检」那条
//   ⇒ 同一份代码一次绿一次红（⑦ 也同理会**假红**：页面还认为锁着 ⇒ 合法的一笔被拦）。
//
// **口径**：
//   · 先擦 `localStorage` 偏好再进文档 ✓ 并**核实**「服务器渲染」勾选 ✓（擦不动 ⇒ `exit(3)` ✓）；
//   · 每个场景用**各自的图层** ⇒ 断言互不干扰 ✓；
//   · 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)`（AGENTS.md：不许把环境问题判成产品红 ✓）；
//   · 用法错误 ⇒ `process.exit(2)` ✓。
//
// 用法：node scripts/browser-layer-lock.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-layer-lock.mjs http://127.0.0.1:15900 9490
import { setTimeout as sleep } from "node:timers/promises";

const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 480000);

const url = process.argv[2];
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
if (!url) {
  console.error("用法: node scripts/browser-layer-lock.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const base = new URL(url).origin;
const shotsDir = process.env.SHOTS_DIR || "/tmp/yanshi-layer-lock-shots";
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};
// **只废本条断言的前提 ⇒ 不许把整条判据一起掩掉** ✓（MUT-F 实测到的 ✓）：⑨ 的「本地内核里得有历史」
//   立不住时 ✗ 直接 `exit(3)` 会让**后面还没跑的 ⑪**一起消失 ✗ 那条本来要抓的红就没被抓到 ✗
//   ⇒ 改成：记一笔 ⇒ 其余照跑 ⇒ 收尾仍以退出码 3 交差（不冒充全绿 ✓ 也不把产品红压成环境问题 ✓）。
const judgeBroken = [];

// ---------- 一次性文档 + 夹具 ----------
const docId = "t_layerlock_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 240, height: 180, background: { r: 255, g: 255, b: 255, a: 255 } }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());

const LOCKED = "layer_locked_paint";
const OTHER = "layer_other";
for (const id of [LOCKED, OTHER]) {
  const made = await callTool("create_layer", { layer_id: id, name: id === LOCKED ? "lock-target" : "other-layer" });
  if (!made.ok) { console.error("❌ 判据前提失败：建图层 " + id + " ⇒ " + JSON.stringify(made).slice(0, 160)); process.exit(3); }
}

/** `get_log` 的 `count` 是"本次返回了几行" ✓ ⇒ **不给 `limit`**（给了就恒真 ✗）。 */
const atomCount = async () => Number((await callTool("get_log", {})).count || 0);
const layersOf = async () => (await callTool("list_layers", {})).layers || [];
const objectsOf = async (layerId) => {
  const found = (await layersOf()).find((l) => l.layer_id === layerId);
  return found ? Number(found.objects) : NaN;
};
const lockedOf = async (layerId) => {
  const found = (await layersOf()).find((l) => l.layer_id === layerId);
  return found ? !!found.locked : null;
};
/** 页面**自己**认不认这笔锁：那颗按钮的字形只有 🔒／🔓 ✓ 与界面语言无关 ✓。
 *  为什么必须等它而不能只等服务端（第六轮实测抓到的飘 ✓）：预检读的是**页面里的锁缓存** ✓
 *  而这份缓存在 `renderLayerPanel` 顶部重建 ⇒ 与那颗按钮的字形是**同一次渲染**的产物 ✓。
 *  判据先前只 poll 服务端 ⇒ 服务端已锁、页面尚未刷新时落笔 ⇒ 请求照发、被后端 403 兜住
 *  ⇒ 测到的是「兜底」那条 ✗ 不是「预检」那条 ⇒ 同一份代码一次绿一次红。
 *  等的是**前提**（页面知道这层锁了）✗ 不是结论 ⇒ 不削弱任何断言 ✓。 */
const pageLockGlyph = (layerId) => evaluate(`(() => {
  const row = document.querySelector('.layer-row[data-layer-id="${layerId}"]');
  const btn = row && row.querySelector('button[data-action="locked"]');
  return btn ? btn.textContent : null;
})()`);
async function waitLockView(layerId, wantLocked, what) {
  const want = wantLocked ? "🔒" : "🔓";
  let seen = null;
  for (let i = 0; i < 40; i += 1) {
    seen = await pageLockGlyph(layerId);
    if (seen === want) break;
    await sleep(250);
  }
  if (seen !== want) {
    console.error("❌ 判据前提失败：" + what + "之后 ✗ 界面自己没认账（那颗按钮停在 " + JSON.stringify(seen)
      + " ⇐ 期望 " + JSON.stringify(want) + "）⇒ 点完按钮界面不重绘 ⇒ 后面测不到这条路");
    process.exit(3);
  }
  const server = await lockedOf(layerId);
  if (server !== wantLocked) {
    console.error("❌ 判据前提失败：界面说 " + JSON.stringify(seen) + " ⇒ 服务端却说 locked:"
      + JSON.stringify(server) + " ⇒ 两边对不上 ⇒ 测的是分叉，不是产品结论");
    process.exit(3);
  }
  return seen;
}

// ① **服务端前提**：锁定层上落笔真的会被拒 ✗ 否则本判据测的是另一条路。
const preLock = await callTool("update_layer", { layer_id: LOCKED, patch: { locked: true } });
if (!preLock.ok) { console.error("❌ 判据前提失败：HTTP 锁层 ⇒ " + JSON.stringify(preLock).slice(0, 160)); process.exit(3); }
const refused = await callTool("draw_stroke", {
  layer_id: LOCKED, data: { points: [[40, 40], [90, 80]], size: 10, color: "#ff0000", smooth: false },
});
if (refused.ok || refused.error_code !== "permission_denied") {
  console.error("❌ 判据前提失败：锁定层这次没被拒 ⇒ " + JSON.stringify(refused).slice(0, 200));
  process.exit(3);
}
ok(`① 服务端前提：锁定层落笔回 403 ${refused.error_code}`, String((refused.context && refused.context.detail) || "").slice(0, 60));
// 同一前提的另一半：**蒙版那条不许被拒**（⑥ 的依据 ✗ 若哪天服务端改成拒绝 ✓ 这条会先告诉我们 ✓）。
const maskProbe = await callTool("create_mask", {
  mask_id: "mask_probe_ok", shape: { kind: "rect", x: 1, y: 1, w: 8, h: 8 }, feather: 0, linked_layer: LOCKED,
});
if (!maskProbe.ok) {
  console.error("❌ 判据前提失败：`create_mask` 对锁定层这次**也被拒了**（" + JSON.stringify(maskProbe).slice(0, 160)
    + "）⇒ 界面该不该拦的口径变了 ⇒ 先改判据抬头再动产品");
  process.exit(3);
}
ok("⑥ 前提：`create_mask` 对锁定层是放行的 ⇒ 界面不许拦它", maskProbe.status === undefined ? "ok" : "ok");
// 夹具回到「未锁」的状态：② 要用**界面上那颗 🔒** 去锁 ✓ 不能拿 HTTP 后门代替 ✓。
const unlockPre = await callTool("update_layer", { layer_id: LOCKED, patch: { locked: false } });
if (unlockPre.ok !== true) { console.error("❌ 判据前提失败：把夹具解锁 ⇒ " + JSON.stringify(unlockPre).slice(0, 160)); process.exit(3); }
const atomsStart = await atomCount();
if (!(atomsStart > 0)) { console.error("❌ 判据前提失败：读不到原子计数"); process.exit(3); }

// ---------- CDP ----------
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）"); process.exit(3); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
let msgId = 0;
const pending = new Map();
const eventWaiters = [];
const pageErrors = [];
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
  for (let i = eventWaiters.length - 1; i >= 0; i -= 1) {
    if (eventWaiters[i].method === msg.method) { eventWaiters[i].resolve(msg); eventWaiters.splice(i, 1); }
  }
  if (msg.method === "Runtime.exceptionThrown") {
    pageErrors.push(String(msg.params.exceptionDetails?.exception?.description || msg.params.exceptionDetails?.text || "unknown").slice(0, 160));
  }
  if (msg.method === "Log.entryAdded" && msg.params.entry?.level === "error" && msg.params.entry?.source === "javascript") {
    pageErrors.push(String(msg.params.entry.text).slice(0, 160));
  }
  // ⑫ 用的**请求改写**钩子 ✓（CDP `Fetch` ✓ 把 `/api/atoms` 换成一份**内核必定装不下**的原子）：
  if (msg.method === "Fetch.requestIntercepted" && onIntercepted) onIntercepted(msg.params);
});
await new Promise((res, rej) => { ws.addEventListener("open", res); ws.addEventListener("error", rej); });
const send = (method, params = {}) => new Promise((res) => {
  const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params }));
});
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
/** `Runtime.evaluate` 必须 `returnByValue` + `awaitPromise` 成对 ✓（AGENTS.md ✓）
 *  而读数走 `res.result.result.value` ✗ 不是 `res.result.value` ✓。 */
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}
const capture = async (name) => {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const file = `${shotsDir}/${name}.png`;
  await fs.writeFile(file, Buffer.from(data, "base64"));
  return file;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
await send("Log.enable");

// ---------- 读数用的三段页面代码 ----------
const READ_NOTICE = `(() => {
  const el = document.getElementById("stageNotice");
  if (!el) return { exists: false };
  const r = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  return { exists: true, hidden: el.hidden === true, text: el.textContent || "",
    w: Math.round(r.width), h: Math.round(r.height),
    inViewport: r.top >= 0 && r.bottom <= (window.innerHeight || 0) && r.width > 0 && r.height > 0,
    display: cs.display, opacity: cs.opacity, pointerEvents: cs.pointerEvents };
})()`;
// 画布上「非白」像素的数量 ✓ 只看有没有变化 ✗ 不假设笔刷颜色 ✓。
const READ_INK = `(() => {
  const b = document.getElementById("board");
  if (!b || !b.width) return null;
  const ctx = b.getContext("2d", { willReadFrequently: true });
  const d = ctx.getImageData(0, 0, b.width, b.height).data;
  let ink = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] === 0) continue;
    if (d[i] > 244 && d[i + 1] > 244 && d[i + 2] > 244) continue;
    ink += 1;
  }
  return ink;
})()`;
const SELECT_LAYER = `(async (id) => {
  const sel = document.getElementById("layer");
  if (!sel) return "NO-SELECT";
  sel.value = id;
  if (sel.value !== id) return "MISSING-OPTION";
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  sel.onchange && await sel.onchange();
  return (window.yanshi.state() || {}).layerId || String(sel.value);
})`;
const CLICK_TOOL = `(async (tool) => {
  const b = document.querySelector('button[data-tool="' + tool + '"]');
  if (!b) return "NO-BUTTON";
  b.click();
  return (window.yanshi.state() || {}).tool;
})`;

/** 在画布上真拖一把（CDP 的**鼠标事件** ✓ 不是 `dispatchEvent` 合成的 ✗ 那样测不到命中与捕获）。 */
async function dragOnBoard(fraction) {
  const box = await evaluate(`(() => {
    const r = document.getElementById("board").getBoundingClientRect();
    return { x: r.left + r.width * ${fraction.x}, y: r.top + r.height * ${fraction.y},
             dx: r.width * ${fraction.dx}, dy: r.height * ${fraction.dy} };
  })()`);
  if (!box) return false;
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", buttons: 1, clickCount: 1 });
  await sleep(90);
  for (let step = 1; step <= 6; step += 1) {
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: box.x + (box.dx * step) / 6, y: box.y + (box.dy * step) / 6, button: "left", buttons: 1,
    });
    await sleep(60);
  }
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x + box.dx, y: box.y + box.dy, button: "left", buttons: 0, clickCount: 1 });
  await sleep(900);
  return true;
}
async function clickOnBoard(fraction) {
  const p = await evaluate(`(() => {
    const r = document.getElementById("board").getBoundingClientRect();
    return { x: r.left + r.width * ${fraction.x}, y: r.top + r.height * ${fraction.y} };
  })()`);
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: p.x, y: p.y, button: "left", buttons: 1, clickCount: 1 });
  await sleep(80);
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: p.x, y: p.y, button: "left", buttons: 0, clickCount: 1 });
  await sleep(700);
}
/** 浮层那句「锁定」文案的中/英文形状（只认句式 ✗ 不许认「日志里有」✓）。 */
const isLockSentence = (text) => /是锁定图层/.test(text) || /is a locked layer/.test(text);

/** **请求计数器** ✓（本轮变异测试逼出来的盲区 ✓）：只盯"服务端状态没变"⇒ **抽掉预检后 ③ 照样绿** ✗
 *  因为后端本来就拒 ⇒ 客户端"明知会被拒还跑一趟"这一整类缺陷**判不出来** ✓。
 *  ⇒ 于是把「这一次到底有没有把请求发出去」也变成可断言的事实 ✓：名字以 `list_` / `get_` / `render_`
 *  开头的是**读** ✓ 其余算**改** ✓。计数器**自己也要被证明数得到东西** ✓ ⇒ ⑥⑦ 那两把真提交必须让它 >0 ✗
 *  否则"恒 0"就是假绿 ✓（第 887 轮同一条教训 ✓：等前提、不等结论 ✗ 但计数器要自检 ✓）。 */
const NET_HOOK = `(() => {
  if (!window.__netPatched) {
    window.__netPatched = 1;
    window.__netLog = [];
    const real = window.fetch;
    window.fetch = function (input) {
      try {
        const url = typeof input === "string" ? input : (input && input.url) || "";
        const hit = url.match(/\\/api\\/tools\\/([a-z0-9_]+)/);
        if (hit) window.__netLog.push(hit[1]);
      } catch (_) { /* 计数失败绝不能弄坏真实请求 ✓ */ }
      return real.apply(this, arguments);
    };
  }
  window.__netLog = [];
  return 1;
})()`;
const netWrites = () => evaluate(`(window.__netLog || []).filter((n) => !/^(list_|get_|render_)/.test(n))`);

/** **把"图层名"那一段从英文句子里摘出去** ✓：中英文两种句式分别是 `「名」是锁定图层…` /
 *  `"name" is a locked layer…` ⇒ 引号里的名字是**服务端数据** ✗ 不参与"框架有没有漏汉字"的判定 ✓。
 *  摘不到引号段时**整句当框架** ✗ 宁可误报也不放过真缺陷 ✓。 */
function enFrame(text) {
  const t = String(text || "");
  const m = /^[「"]([^」"]*)[」"]([\s\S]*)$/.exec(t);
  if (!m) return { text: t, dataCjk: [] };
  return { text: m[2], dataCjk: (m[1].match(/[\u4e00-\u9fff]/g) || []) };
}

// ---------- 开机（服务端渲染）----------
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(600);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); localStorage.removeItem("yanshi.lang"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let booted = false;
for (let i = 0; i < 80 && !booted; i += 1) {
  booted = await evaluate(`!!(window.yanshi && window.yanshi.state && document.getElementById("layerList")?.children.length >= 2)`);
  if (!booted) await sleep(250);
}
if (!booted) { console.error("❌ 判据前提失败：图层面板没装着 2 个以上场景图层 ⇒ 页面脚本或 `list_layers` 没就绪"); process.exit(3); }
let serverRenderOn = null;
for (let i = 0; i < 80; i += 1) {
  serverRenderOn = await evaluate(`(() => {
    if (!location.search.includes("render=server")) return "WRONG-PAGE";
    const b = document.getElementById("useServerRender");
    return b ? (b.checked === true ? true : false) : null;
  })()`);
  if (serverRenderOn === true) break;
  await sleep(250);
}
if (serverRenderOn !== true) {
  console.error("❌ 判据前提失败：「服务器渲染」勾选没生效（实为 " + JSON.stringify(serverRenderOn) + "）⇒ 后面读的是本地路由，不是产品结论");
  process.exit(3);
}
await evaluate(`(${SELECT_LAYER})(${JSON.stringify(LOCKED)})`);
await sleep(400);
if ((await evaluate(`(window.yanshi.state() || {}).layerId`)) !== LOCKED) {
  console.error("❌ 判据前提失败：界面上选不中「锁定层」那条图层 ⇒ 后面测的不是目标图层");
  process.exit(3);
}
const toolPicked = await evaluate(`(${CLICK_TOOL})("brush")`);
if (toolPicked !== "brush") { console.error("❌ 判据前提失败：工具条选不中画笔（实为 " + JSON.stringify(toolPicked) + "）"); process.exit(3); }

// ---------- ② 用界面上那颗 🔒 锁起来 ----------
const atomsBeforeLock = await atomCount();
const lockClicked = await evaluate(`(() => {
  const row = document.querySelector('.layer-row[data-layer-id="${LOCKED}"]');
  if (!row) return "NO-ROW";
  const btn = row.querySelector('button[data-action="locked"]');
  if (!btn) return "NO-LOCK-BUTTON";
  btn.click();
  return "clicked";
})()`);
if (lockClicked !== "clicked") {
  console.error("❌ 判据前提失败：图层行里没有那颗 🔒（" + JSON.stringify(lockClicked) + "）⇒ 测不到「界面锁层」这条路");
  process.exit(3);
}
let lockedNow = null;
for (let i = 0; i < 40; i += 1) {
  lockedNow = await lockedOf(LOCKED);
  if (lockedNow === true) break;
  await sleep(250);
}
if (lockedNow !== true) { console.error("❌ 判据前提失败：点了 🔒 之后服务端仍说这层没锁（实为 " + JSON.stringify(lockedNow) + "）"); process.exit(3); }
await waitLockView(LOCKED, true, "点了 🔒");
const atomsAfterLock = await atomCount();
ok("② 图层面板那颗 🔒 走的是 `update_layer`（服务端说 locked:true）",
  { 原子数: atomsBeforeLock + " → " + atomsAfterLock });

// ---------- ③④ 锁定层上画一笔 ----------
const atoms0 = await atomCount();
const objects0 = await objectsOf(LOCKED);
const ink0 = await evaluate(READ_INK);
if (ink0 === null) { console.error("❌ 判据前提失败：读不到 `#board` 的像素 ⇒ 断不了「画布上没多东西」这一条"); process.exit(3); }
await evaluate(`(() => { const el = document.getElementById("stageNotice"); if (el) { el.hidden = true; el.textContent = ""; } return 1; })()`);
await evaluate(NET_HOOK);
const dragged = await dragOnBoard({ x: 0.25, y: 0.4, dx: 0.3, dy: 0.22 });
if (!dragged) { console.error("❌ 判据前提失败：拿不到画布矩形 ⇒ 拖不出这一笔"); process.exit(3); }
const atoms1 = await atomCount();
const objects1 = await objectsOf(LOCKED);
const ink1 = await evaluate(READ_INK);
const notice1 = await evaluate(READ_NOTICE);
const writes1 = await netWrites();
if (atoms1 !== atoms0) bad(`③ 锁定层上落笔**多出了原子**：${atoms0} ⇒ ${atoms1}（预检没拦住 ⇒ 请求真的出门了）`, notice1 && notice1.text);
else ok(`③ 锁定层上落笔一个原子都没多（${atoms0} 保持不变 ✓ 对象数 ${objects0} ⇒ ${objects1} ✓）`);
if (writes1.length !== 0) bad(`③ 这一笔照样把**注定被拒**的请求发出去了（发出 ${JSON.stringify(writes1)}）⇒ 后端兜得住不等于界面可以不拦`, null);
else ok("③ 预检在**出门前**就拦住了（这一把没有任何写入请求发往服务端 ✓ 不是「发出去再被拒」✓）");
if (!(ink0 >= 0) || ink1 !== ink0) bad(`③ 画布像素被改了（非白像素 ${ink0} ⇒ ${ink1}）⇒ 拦下了却还留下像素 = 幻影 ✗`, null);
else ok(`③ 画布逐块相同（非白像素停在 ${ink0} ✓ 一笔都没落 ✗）`);
if (!notice1 || !notice1.exists) bad("④ 页面上根本没有 `#stageNotice` 这个浮层 ⇒ 消息无处说 ✗");
else if (notice1.hidden || notice1.w <= 0 || notice1.h <= 0 || !notice1.inViewport) bad("④ 浮层存在但**看不见**（收起 / 零尺寸 / 不在视口内）", notice1);
else if (!isLockSentence(notice1.text)) bad("④ 浮层说的话与「锁定图层」无关 ⇒ 说的不是这件事", notice1.text);
else if (notice1.pointerEvents === "none") ok(`④ 浮层看得见、说的是锁定那句、且**不吃掉落笔**（pointer-events:none ✓）`, notice1.text);
else { ok(`④ 浮层看得见、说的就是那句锁定解释`, notice1.text); bad("④ 浮层吃鼠标事件 ⇒ 它会挡住落笔 ✗", notice1.pointerEvents); }
await capture("01-zh-locked-refused");

// ---------- ⑤ 文本工具同样被拦（另一条分支）----------
const textTool = await evaluate(`(${CLICK_TOOL})("text")`);
if (textTool !== "text") { console.error("❌ 判据前提失败：选不中文本工具（实为 " + JSON.stringify(textTool) + "）"); process.exit(3); }
const atoms2 = await atomCount();
await evaluate(`(() => { window.__promptCalls = 0; const p = window.prompt; window.prompt = () => { window.__promptCalls += 1; return "hello"; }; return 1; })()`);
await evaluate(NET_HOOK);
await clickOnBoard({ x: 0.6, y: 0.6 });
const atoms3 = await atomCount();
const notice2 = await evaluate(READ_NOTICE);
const writes2 = await netWrites();
const promptCalls = await evaluate(`window.__promptCalls`);
if (atoms3 !== atoms2) bad(`⑤ 锁定层上点文本工具**多出了原子**：${atoms2} ⇒ ${atoms3}`);
else ok(`⑤ 文本那条分支也没落进历史（原子数停在 ${atoms2} ✓）`);
if (promptCalls !== 0) bad(`⑤ 拦得不够早：还是进了文本编辑流程（` + "`prompt`" + ` 被调了 ${promptCalls} 次）⇒ 用户会先看到一个弹窗`, null);
else ok("⑤ 文本被拦在**弹窗之前**（`prompt` 一次都没被调 ✓ 根本不该进编辑流程 ✓）");
if (writes2.length !== 0) bad(`⑤ 文本这一笔照样发出了写入请求（${JSON.stringify(writes2)}）`, null);
else ok("⑤ 文本这一把同样没发出请求 ✓（另一条分支 ✓ 不走通用落笔那条 ✓）");
if (!notice2 || notice2.hidden || !isLockSentence(notice2.text)) bad("⑤ 文本被拦时浮层没在说锁定那句", notice2);
else ok("⑤ 文本被拦时浮层同样看得见", notice2.text);

// ---------- ⑥ 不许误伤：锁着也能挂蒙版 ----------
await evaluate(`(() => { const el = document.getElementById("stageNotice"); if (el) { el.hidden = true; el.textContent = ""; } return 1; })()`);
const maskTool = await evaluate(`(${CLICK_TOOL})("mask_rect")`);
if (maskTool !== "mask_rect") { console.error("❌ 判据前提失败：选不中「矩形蒙版」（实为 " + JSON.stringify(maskTool) + "）"); process.exit(3); }
const atoms4 = await atomCount();
await evaluate(NET_HOOK);
await dragOnBoard({ x: 0.3, y: 0.3, dx: 0.25, dy: 0.25 });
const atoms5 = await atomCount();
const writes3 = await netWrites();
const notice3 = await evaluate(READ_NOTICE);
// **计数器自检** ✓：这一把**确实**落了原子 ⇒ 它必然发过请求 ✗ 数不到就是钩子失效 ✓
//   ⇒ 那么 ③⑤ 的「0 个请求」毫无意义 ⇒ **判据自身失效** ⇒ `exit(3)`（不许冒充产品绿 ✓）。
if (atoms5 > atoms4 && writes3.length === 0) {
  console.error("❌ 判据前提失败：原子涨了却一个 `/api/tools/*` 写入都没数到 ⇒ 请求计数器已失效（端点改路了？）⇒ ③⑤ 的「没发出请求」不可信");
  process.exit(3);
}
const maskRefused = (notice3 && !notice3.hidden && isLockSentence(notice3.text)) === true;
if (atoms5 <= atoms4) bad(`⑥ 锁着的图层上「矩形蒙版」这次**没落原子**（${atoms4} ⇒ ${atoms5}）⇒ 预检把服务端放行的一件事给拦了 ✗`, notice3 && notice3.text);
else if (maskRefused) bad("⑥ 蒙版本来放行，浮层却在说锁定那句 ⇒ 拦错了 ✗", notice3.text);
else ok(`⑥ 没误伤：` + "`create_mask`" + ` 对锁定层放行 ✓ 这一把真的落了原子（${atoms4} ⇒ ${atoms5} ✓ 请求照常出门 ${JSON.stringify(writes3)}）`);
await capture("02-zh-mask-still-works");

// ---------- ⑦ 解锁后同一把必须画得上（证明 ③ 不是「这工具坏了」的假绿）----------
await evaluate(`(${CLICK_TOOL})("brush")`);
const unlockedClick = await evaluate(`(() => {
  const row = document.querySelector('.layer-row[data-layer-id="${LOCKED}"]');
  const btn = row && row.querySelector('button[data-action="locked"]');
  if (!btn) return "NO-BUTTON";
  btn.click();
  return "clicked";
})()`);
if (unlockedClick !== "clicked") { console.error("❌ 判据前提失败：点不开那颗锁按钮（" + JSON.stringify(unlockedClick) + "）"); process.exit(3); }
for (let i = 0; i < 40; i += 1) { if ((await lockedOf(LOCKED)) === false) break; await sleep(250); }
if ((await lockedOf(LOCKED)) !== false) { console.error("❌ 判据前提失败：点解锁之后服务端还说这层锁着"); process.exit(3); }
await waitLockView(LOCKED, false, "点了「解锁」");
await evaluate(`(() => { const el = document.getElementById("stageNotice"); if (el) { el.hidden = true; el.textContent = ""; } return 1; })()`);
const atoms6 = await atomCount();
await evaluate(NET_HOOK);
await dragOnBoard({ x: 0.25, y: 0.4, dx: 0.3, dy: 0.22 });
const atoms7 = await atomCount();
const writes4 = await netWrites();
const ink7 = await evaluate(READ_INK);
if (atoms7 <= atoms6) bad(`⑦ 解锁后同一笔**没落原子**（${atoms6} ⇒ ${atoms7}）⇒ 那 ③ 的「画不上」就可能是工具坏了的假绿 ✗`);
else if (!(ink7 > ink0)) bad(`⑦ 解锁后落了原子（${atoms6} ⇒ ${atoms7}）但画布上没有墨（非白像素 ${ink0} ⇒ ${ink7}）⇒ 拦与放都验不到像素那一层 ✗`);
else if (writes4.length === 0) bad(`⑦ 解锁后落了原子却没数到写入请求（${JSON.stringify(writes4)}）⇒ 拦/放这条轴失去证据 ✗`);
else ok(`⑦ 解锁后同一笔照画：原子 ${atoms6} ⇒ ${atoms7} ✓ 画布非白像素 ${ink0} ⇒ ${ink7} ✓ 请求照常出门 ${JSON.stringify(writes4)}`);
await capture("03-zh-unlocked-painted");

// ---------- ⑧ 英文模式下那句话必须是英文 ----------
await evaluate(`(() => { localStorage.setItem("yanshi.lang", "en"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=en&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let enBooted = false;
for (let i = 0; i < 80 && !enBooted; i += 1) {
  enBooted = await evaluate(`!!(window.yanshi && document.getElementById("layerList")?.children.length >= 2)`);
  if (!enBooted) await sleep(250);
}
if (!enBooted) { console.error("❌ 判据前提失败：英文模式开不起来"); process.exit(3); }
await evaluate(`(${SELECT_LAYER})(${JSON.stringify(LOCKED)})`);
await sleep(300);
const relock = await evaluate(`(() => {
  const row = document.querySelector('.layer-row[data-layer-id="${LOCKED}"]');
  const btn = row && row.querySelector('button[data-action="locked"]');
  if (!btn) return "NO-BUTTON";
  btn.click();
  return "clicked";
})()`);
if (relock !== "clicked") { console.error("❌ 判据前提失败：英文模式下点不到那颗锁按钮"); process.exit(3); }
for (let i = 0; i < 40; i += 1) { if ((await lockedOf(LOCKED)) === true) break; await sleep(250); }
if ((await lockedOf(LOCKED)) !== true) { console.error("❌ 判据前提失败：英文模式下锁没生效"); process.exit(3); }
await waitLockView(LOCKED, true, "英文模式下点了锁");
await evaluate(`(${CLICK_TOOL})("brush")`);
const atomsEn = await atomCount();
await dragOnBoard({ x: 0.55, y: 0.3, dx: 0.2, dy: 0.2 });
const atomsEn2 = await atomCount();
const noticeEn = await evaluate(READ_NOTICE);
if (atomsEn2 !== atomsEn) bad(`⑧ 英文模式下这次**多出了原子**（${atomsEn} ⇒ ${atomsEn2}）⇒ 拦不住`, noticeEn && noticeEn.text);
if (!noticeEn || noticeEn.hidden || !isLockSentence(noticeEn.text)) bad("⑧ 英文模式下浮层没说那句锁定解释", noticeEn);
// **汉字分两种口径** ✓（与 `browser-fill-region.mjs` 同一条 ✓）：**图层名是服务端数据** ✓
//（用户自己也能起中文名 ✗）⇒ 不翻 ✓ 只有**句式框架**漏出汉字才是词条缺失 ✗。
const frame = enFrame(noticeEn && noticeEn.text);
if (/[\u4e00-\u9fff]/.test(frame.text)) bad("⑧ 英文模式下浮层的**句式框架**漏出汉字 ⇒ I18N 词条缺了 ✗", frame.text);
else ok("⑧ 英文模式下浮层是整句英文（框架 0 个汉字 ✓）", noticeEn && noticeEn.text);
if (frame.dataCjk.length > 0) console.log("  （同一句里另有 " + frame.dataCjk.length + " 个汉字属于**服务端数据**（图层名）⇒ 不翻 ✓）");
if (noticeEn && noticeEn.exists && noticeEn.inViewport !== true) bad("⑧ 英文模式下浮层不在视口内（看不见等于没说）", noticeEn);
await capture("04-en-locked-refused");

// ---------- ⑨ 本地内核模式下也不许留幻影 ----------
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=client&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
await sleep(900);
const kernelReady = await evaluate(`(() => {
  if (!window.yanshi || !window.yanshi.kernelStats) return "NO-API";
  const b = document.getElementById("useServerRender");
  return b && b.checked === false ? true : "CHECKBOX";
})()`);
if (kernelReady !== true) {
  console.log("  ⚠ 本地内核模式起不来（" + JSON.stringify(kernelReady) + "）⇒ 跳过 ⑨（这是环境问题 ⇒ 不算产品红 ✗）");
} else {
  await evaluate(`(${SELECT_LAYER})(${JSON.stringify(LOCKED)})`);
  await sleep(300);
  await evaluate(`(${CLICK_TOOL})("brush")`);
  const serverNow = await atomCount();
  // **⑨ 自己的前提** ✓（run2 实测逼出来的 ✓）：`render=client` 刚导航完时内核可能**还没把历史搬进来** ✓
  //   ⇒ 读到 `atoms:0` ⇒ 「一个原子都没多」变成 **0 ⇒ 0 的真空断言** ✗ 那种绿是假的 ✓。
  //   ⇒ 有界等到「内核里确实有历史」才认 ✗ 等不到 ⇒ **判据自身失效** ⇒ `exit(3)`（不冒充产品绿 ✓）。
  let localAtoms0 = null;
  for (let i = 0; i < 40; i += 1) {
    localAtoms0 = await evaluate(`(() => { try { return window.yanshi.kernelStats().atoms; } catch (_) { return null; } })()`);
    if (typeof localAtoms0 === "number" && localAtoms0 > 0) break;
    await sleep(250);
  }
  if (!(localAtoms0 > 0)) {
    judgeBroken.push("⑨ 本地内核里一个原子都没有（实为 " + JSON.stringify(localAtoms0) +
      "，服务端那边是 " + serverNow + "）⇒ 「幻影」那条断言成了 0⇒0 的空判 ⇒ 本条不作数");
    console.log("  ❌ 判据自身：⑨ 的前提立不住 ⇒ 跳过 ⑨ 的三条断言（其余照跑 ✗ 收尾退出码 3 ⇒ 不冒充全绿 ✓）");
  } else {
  const inkLocal0 = await evaluate(READ_INK);
  await evaluate(`(() => { const el = document.getElementById("stageNotice"); if (el) { el.hidden = true; el.textContent = ""; } return 1; })()`);
  await dragOnBoard({ x: 0.2, y: 0.65, dx: 0.3, dy: 0.15 });
  const localAtoms1 = await evaluate(`(() => { try { return window.yanshi.kernelStats().atoms; } catch (_) { return null; } })()`);
  const inkLocal = await evaluate(READ_INK);
  const noticeLocal = await evaluate(READ_NOTICE);
  if (localAtoms1 !== localAtoms0) bad(`⑨ 本地内核里多出了原子（${localAtoms0} ⇒ ${JSON.stringify(localAtoms1)}）⇒ 预检没拦住本地那一笔 ✗`);
  else ok(`⑨ 本地内核一个原子都没多（停在 ${localAtoms0} ✓ 服务端此刻是 ${serverNow} ✓ 两边都不长 ✗ 没有幻影原子 ✓）`);
  if (inkLocal !== inkLocal0) bad(`⑨ 本地模式下画布被改了（非白像素 ${inkLocal0} ⇒ ${inkLocal}）⇒ 幻影像素 ✗`, null);
  else ok(`⑨ 本地模式下画布也没多一笔（非白像素停在 ${inkLocal0} ✓ 服务端没有的东西画布上也不许有 ✓）`);
  if (!noticeLocal || noticeLocal.hidden || !isLockSentence(noticeLocal.text)) bad("⑨ 本地模式下浮层没说锁定那句", noticeLocal);
  else ok("⑨ 本地模式下同样当面谈清楚", noticeLocal.text);
  await capture("05-client-locked-refused");
  }
}

// ---------- ⑪ 锁是**另一头**上的：界面一开始不知道，被拒之后必须自己认账 ----------
// **为什么单独立一条** ✓（⑧ 那一把飘出来的 ✓ 不是设计出来的）：预检读的是**页面里的缓存** ✓
//   而同事／MCP 那边把层锁了 ⇒ 这份缓存从来没更新过 ⇒ 预检根本不会响 ✗ 每一笔都发一个
//   注定被拒的请求 ✗ 图层行上一直挂着 🔓 ⇒ **界面在说谎**。
//   判据先前只 poll 服务端 ⇒ 同一段代码一次绿一次红 ✗ 这条把「页面知不知道」当**前提**测清楚 ✓。
await callTool("update_layer", { layer_id: LOCKED, patch: { locked: false } });
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let booted11 = false;
for (let i = 0; i < 80 && !booted11; i += 1) {
  booted11 = await evaluate(`!!(window.yanshi && document.getElementById("layerList")?.children.length >= 2)`);
  if (!booted11) await sleep(250);
}
if (!booted11) { console.error("❌ 判据前提失败：⑪ 这一页起不来"); process.exit(3); }
await evaluate(`(${SELECT_LAYER})(${JSON.stringify(LOCKED)})`);
await sleep(300);
await evaluate(`(${CLICK_TOOL})("brush")`);
if ((await pageLockGlyph(LOCKED)) !== "🔓") {
  console.error("❌ 判据前提失败：⑪ 开局界面就认为这层是锁着的 ⇒ 构造不出「另一头锁的」这条路");
  process.exit(3);
}
// 只从 API 上锁 ⇒ 页面**不该**知道
const peerLock = await callTool("update_layer", { layer_id: LOCKED, patch: { locked: true } });
if (!peerLock.ok) { console.error("❌ 判据前提失败：⑪ 从 API 锁层没成 ⇒ " + JSON.stringify(peerLock).slice(0, 120)); process.exit(3); }
let peerLocked = null;
for (let i = 0; i < 40; i += 1) { peerLocked = await lockedOf(LOCKED); if (peerLocked === true) break; await sleep(250); }
if (peerLocked !== true) { console.error("❌ 判据前提失败：⑪ 服务端没锁上"); process.exit(3); }
const unaware = await pageLockGlyph(LOCKED);
if (unaware !== "🔓") {
  console.error("❌ 判据前提失败：⑪ 还没落笔界面就已经知道了（glyph=" + JSON.stringify(unaware) +
    "）⇒ 「界面不知道」这条路构造不出来 ✗ 测不到东西");
  process.exit(3);
}
const atoms11a = await atomCount();
const ink11a = await evaluate(READ_INK);
await evaluate(`(() => { const el = document.getElementById("stageNotice"); if (el) { el.hidden = true; el.textContent = ""; } return 1; })()`);
await evaluate(NET_HOOK);
await dragOnBoard({ x: 0.3, y: 0.75, dx: 0.25, dy: 0.12 });
const atoms11b = await atomCount();
const ink11b = await evaluate(READ_INK);
const notice11 = await evaluate(READ_NOTICE);
const writes11 = await netWrites();
if (atoms11b !== atoms11a) bad(`⑪ 另一头锁的层上落笔居然多了原子（${atoms11a} ⇒ ${atoms11b}）⇒ 后端兜底也没拦住 ✗`, writes11);
else ok(`⑪ 另一头锁的层上一笔都没落进历史（停在 ${atoms11a} ✓ 后端兜住了 ✓）`);
if (ink11b !== ink11a) bad(`⑪ 画布被改了（非白像素 ${ink11a} ⇒ ${ink11b}）⇒ 被拒的一笔留下了幻影像素 ✗`);
else ok(`⑪ 画布也没有幻影像素（停在 ${ink11a} ✓）`);
const honest11 = notice11 && notice11.exists && notice11.hidden === false
  && (isLockSentence(notice11.text) || /没有成功|did not go through/.test(notice11.text));
if (!honest11) bad("⑪ 这一把用户既没画上、也没听到任何解释（既不是锁定那句 ✓ 也不是「没有成功」那句 ✗）", notice11);
else ok("⑪ 被拒这一把有话说（锁定那句或「没有成功」那句 ✓ 不是「按了没反应」）", notice11.text);
console.log("  （页面一开始不知道 ⇒ 这一把发出了 " + JSON.stringify(writes11) + " ⇒ 下一笔起必须知道）");
// **必须自己认账**：被拒之后把图层状态读回来 ⇒ 那行 glyph 要翻成 🔒
let converged = null;
for (let i = 0; i < 40; i += 1) {
  converged = await pageLockGlyph(LOCKED);
  if (converged === "🔒") break;
  await sleep(250);
}
if (converged !== "🔒") {
  bad("⑪ 被拒之后界面还一直说「没锁」（glyph 停在 " + JSON.stringify(converged)
    + "）⇒ 图层行在说谎 ✗ 用户会一遍遍发注定被拒的请求");
} else ok("⑪ 被拒之后界面自己认账了：那一行翻成 🔒 ✓（缓存与页面同一时刻 ✓）");
// 认账之后 ✗ 第二笔必须**在出门前**被拦 (与 ③ 同一条口径 ✓)
await evaluate(NET_HOOK);
await evaluate(`(() => { const el = document.getElementById("stageNotice"); if (el) { el.hidden = true; el.textContent = ""; } return 1; })()`);
await dragOnBoard({ x: 0.35, y: 0.8, dx: 0.25, dy: 0.1 });
const writes11b = await netWrites();
const notice11b = await evaluate(READ_NOTICE);
if (writes11b.length !== 0) bad("⑪ 界面已经知道锁了 ✗ 第二笔还是把请求发出去了 ⇒ 预检没接上这份认知", writes11b);
else ok("⑪ 认账之后第二笔一个请求都没发（拦在出门前 ✓ 不再刷注定被拒的 403 ✓）");
if (!notice11b || notice11b.hidden || !isLockSentence(notice11b.text)) bad("⑪ 第二笔没说锁定那句", notice11b);
else ok("⑪ 第二笔当面说清「先解锁」", notice11b.text);
if ((await atomCount()) !== atoms11a) bad("⑪ 兜到最后原子还是变了 ⇒ 上面那几条绿是假的");
await capture("06-peer-lock-converged");

// ---------- ⑫ 本地内核装不下这份文档时：不许白布、不许谎报 HEAD、必须当面谈 ----------
// **注入是故意的** ✓（不是"造个假场景"✗）：本轮实测到 `load_atoms_json` 里的 wasm 陷阱
//   是**间歇**的（4 次全新文档第一次本地打开中 1 次 ✓ 探针 `/tmp/yanshi-web-r6/probe-first-client-open.mjs` ✓）
//   ⇒ 判据不能靠"等它自己坏一次" ✗ 必须把**同一条失败形状**（内核装载抛出来）确定性地喂进去 ✓
//   再看页面的反应。做法 ✓：在页面里给 `window.fetch` 包一层桩 ✓ 把 `GET /api/atoms` 换成一份
//   **任何 Atom 都解析不了**的载荷 ⇒ 产品那条装载路原样走到这份响应上 ✓（CDP `Fetch` 那条试过 ✗ 拦不到 ⇒ 见下面的注释 ✓）。
const docK = "t_kernelload_" + Date.now().toString(36).slice(-6);
const openedK = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docK, width: 240, height: 180, background: { r: 255, g: 255, b: 255, a: 255 } }),
}).then((r) => r.json());
if (!openedK.ok) { console.error("❌ 判据前提失败：⑫ 开文档 ⇒ " + JSON.stringify(openedK).slice(0, 160)); process.exit(3); }
const tokenK = openedK.token;
const callToolK = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docK)}&token=${encodeURIComponent(tokenK)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());
const kLayer = await callToolK("create_layer", { layer_id: "layer_k", name: "kernel-doc-layer" });
if (!kLayer.ok) { console.error("❌ 判据前提失败：⑫ 建图层 ⇒ " + JSON.stringify(kLayer).slice(0, 120)); process.exit(3); }
const kStroke = await callToolK("draw_stroke", {
  layer_id: "layer_k", data: { points: [[20, 20], [120, 90]], size: 10, color: "#123456", smooth: false },
});
if (!kStroke.ok) { console.error("❌ 判据前提失败：⑫ 服务端落一笔 ⇒ " + JSON.stringify(kStroke).slice(0, 120)); process.exit(3); }
// 服务端**自己**得能 render 出画面来 ✓ 否则后面「画布不许空」那条测的是环境不是产品 ✗
const serverRenderCheck = await callToolK("render_region", { region: { x: 0, y: 0, w: 240, h: 180 } });
if (!serverRenderCheck.ok || !serverRenderCheck.thumb_url) {
  console.error("❌ 判据前提失败：⑫ 服务端这条路渲染不出来 ⇒ " + JSON.stringify(serverRenderCheck).slice(0, 160));
  process.exit(3);
}

const BOGUS_ATOMS = JSON.stringify({ ok: true, head_seq: 1, atoms: [{ kind: "not_a_real_kind" }] });
// **注入点 = 页面自己的 `window.fetch`** ✗ 不是 CDP 的 `Fetch` ✓（本轮实测逼出来的 ✓）：
//   **∴ 为什么 ✗**：`Fetch.enable`（urlPattern `*/api/atoms*` ✓ 空标签页也一样 ✓ 探针
//     `/tmp/yanshi-web-r6/probe-fetch-intercept.mjs` ✓）**一条都没拦到** ✓
//     ⇒ 那套"改写"等于**没改写** ✗ 而下面的断言全在**原文档**上跑 ⇒ 那种绿是假的 ✗。
//   **∴ 换的路 ✓**：`Page.addScriptToEvaluateOnNewDocument` ⇒ 在**任何页面脚本之前**把 `window.fetch`
//     包一层 ✓ 产品那条路（`loadKernel` ⇒ `fetchOrLocal` ⇒ `fetch`）**原样**走到这个桩 ✓
//     ⇒ 喂进去的是**内核必定装不下**的载荷 ✓ 判定仍然落在**真实失败形状**上 ✓ 不是给测试开的后门 ✗。
//   **∴ 桩自己也要被检查 ✓**：它把命中次数记进 `window.__stubHits` ✓ 下面拿它当**前提** ✓
//     （"一次都没接到"⇒ 判据自身失效 ⇒ `exit(3)` ✓ 不许冒充产品绿 ✗）。
const STUB_ATOMS =
  "window.__stubHits = 0;" +
  "(function () {" +
  "  const real = window.fetch;" +
  "  window.fetch = function (input, options) {" +
  "    try {" +
  "      const u = String(typeof input === 'string' ? input : (input && input.url) || '');" +
  "      const m = options && options.method ? String(options.method) : 'GET';" +
  "      if (u.indexOf('/api/atoms') >= 0 && m === 'GET') {" +
  "        window.__stubHits = window.__stubHits + 1;" +
  "        return Promise.resolve(new Response(" + JSON.stringify(BOGUS_ATOMS) +
  ", { status: 200, headers: { 'content-type': 'application/json' } }));" +
  "      }" +
  "    } catch (error) { /* 桩自己坏了 ⇒ 落回真实 fetch ⇒ 前提那一条会红 ✓ */ }" +
  "    return real.apply(this, arguments);" +
  "  };" +
  "})();";
const stubAdded = await send("Page.addScriptToEvaluateOnNewDocument", { source: STUB_ATOMS });
const stubId = stubAdded && stubAdded.result ? stubAdded.result.identifier : null;

const READ_FALLBACK = `(() => {
  const el = document.getElementById("stageNotice");
  const box = document.getElementById("useServerRender");
  let ks = null;
  try { ks = window.yanshi && window.yanshi.kernelStats ? window.yanshi.kernelStats() : null; } catch (_) { ks = "THROW"; }
  const stats = window.yanshiStats || {};
  return {
    stubHits: typeof window.__stubHits === "number" ? window.__stubHits : null,
    notice: el && el.hidden !== true ? String(el.textContent || "") : "",
    boxChecked: box ? box.checked === true : null,
    kernelStats: ks,
    kernelHead: stats.kernelHead === undefined ? null : stats.kernelHead,
    serverHead: stats.serverHead === undefined ? null : stats.serverHead,
    failures: stats.kernelLoadFailures === undefined ? 0 : stats.kernelLoadFailures,
    wasm: stats.wasm === true,
    ink: (function () {
      const b = document.getElementById("board");
      if (!b || !b.width) return null;
      const ctx = b.getContext("2d", { willReadFrequently: true });
      const d = ctx.getImageData(0, 0, b.width, b.height).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) { if (d[i + 3] === 0) continue; if (d[i] > 244 && d[i + 1] > 244 && d[i + 2] > 244) continue; n += 1; }
      return n;
    })(),
  };
})()`;

async function openClientBrokenKernel(lang) {
  await send("Page.navigate", { url: `${base}/?doc=${docK}&token=${tokenK}&lang=${lang}&render=client&v=${Date.now()}` });
  await waitEvent("Page.loadEventFired");
  let read = null;
  for (let i = 0; i < 120; i += 1) {
    read = await evaluate(READ_FALLBACK);
    if (read && read.notice && /本地内核没能|could not load this document/.test(read.notice)) break;
    await sleep(250);
  }
  return read;
}

const twelve12 = await openClientBrokenKernel("zh");
// **⑫ 自己的前提** ✓：桩必须真的接到那一次 `GET /api/atoms` ✓ 且这一页拿到了 wasm 模块 ✓
//   否则「退回服务端渲染」那六条断言测的是**别的东西** ✗ ⇒ 记成判据自身失效 ⇒ `exit(3)` ✓ 不冒充产品绿 ✗。
let twelvePremise = true;
if (stubId === null || !(twelve12 && twelve12.stubHits > 0)) {
  judgeBroken.push("⑫ 页面里的 fetch 桩一次都没接到 GET /api/atoms（桩注入 " +
    (stubId === null ? "失败" : "成功") + " ✓ 命中 " + JSON.stringify(twelve12 && twelve12.stubHits) +
    "）⇒ 注入没生效 ⇒ 测的不是这条路（不是产品红 ✓）");
  twelvePremise = false;
} else if (!twelve12.wasm) {
  judgeBroken.push("⑫ 这一页连 wasm 模块都没拿到 ⇒ 走不到「装载失败」那一步（环境/网络问题 ✓ 不是产品红 ✗）");
  twelvePremise = false;
}
if (!twelvePremise) {
  console.log("  ❌ 判据自身：⑫ 的前提立不住 ⇒ 跳过 ⑫ 的七条断言（其余照跑 ✗ 收尾退出码 3 ⇒ 不冒充全绿 ✓）");
} else {
const fellBack = twelve12.notice && /本地内核没能把这份文档装进来/.test(twelve12.notice);
if (!fellBack) bad("⑫ 本地内核装不下这份文档 ⇒ 界面什么都没说的（只剩一块白布 ✗ 用户以为作品没了）", twelve12 && twelve12.notice);
else ok("⑫ 内核装不上时当面谈：说了「改用服务端渲染」✓ 不是只写进收起的日志", twelve12.notice);
if (!twelve12 || twelve12.boxChecked !== true) bad("⑫ 已经退回服务端渲染了 ✗ 那颗「用服务端渲染」的勾选还说不一致 ⇒ 界面自相矛盾", twelve12 && twelve12.boxChecked);
else ok("⑫ 勾选跟着翻成「服务端渲染」✓（当前生效的是哪条就显示哪条 ✓ 而 `localStorage` 里用户的偏好一个字没动 ✗）");
// 服务端像素是 `preview.onload` 之后才铺到 `#board` 的 ✓ 有异步 ⇒ **有界等到画布真有东西** ✗
// 立刻读会把「还在解码」判成「画布空的」✗ 那是判据读早了 ✗ 不是产品的错 ✓（AGENTS.md 同一条口径 ✓）。
for (let i = 0; i < 40; i += 1) {
  const again = await evaluate(READ_FALLBACK);
  if (again && again.ink > 0) { twelve12.ink = again.ink; break; }
  await sleep(250);
}
if (!twelve12 || !(twelve12.ink > 0)) bad("⑫ 退回之后画布还是空的（非白像素 " + JSON.stringify(twelve12 && twelve12.ink) + "）⇒ 「画面照常」是句空话 ✗", null);
else ok("⑫ 退回之后画布真的有画面（非白像素 " + twelve12.ink + " ✓ 服务端那份墨显示出来了 ✓）");
if (twelve12 && twelve12.kernelStats !== null) bad("⑫ 那个装不下的内核还留在手里（kernelStats 非 null ⇒ 后面可能拿它画 ✗）", twelve12.kernelStats);
else ok("⑫ 空内核被回收了 ✓（kernelStats 为 null ⇒ 没人会再拿它产出像素 ✓）");
if (twelve12 && twelve12.kernelHead > 0) {
  bad("⑫ 内核里一条都没装 ✓ 顶栏却报 HEAD=" + JSON.stringify(twelve12.kernelHead) + " ⇒ **谎报同步** ✗ 用户会以为文档损坏了", twelve12);
} else ok("⑫ 没谎报同步：kernelHead 停在 0 ✓（服务端头部一个字都没写进本地状态 ✓）");
if (twelve12 && !(twelve12.failures > 0)) bad("⑫ 这次退回没记账（kernelLoadFailures 为 0）⇒ 判据没法数 ✗ 线上也看不见它发生了多少次", twelve12 && twelve12.failures);
else ok("⑫ 退回记了账：kernelLoadFailures=" + twelve12.failures + " ✓（诊断里看得见 ✗ 不是只有一张脸）");
await capture("07-kernel-fallback-zh");

// 同一件事在**英文界面**里必须也是英文 ✓（新句子的词条漏了就是这里红 ✗ 与 ⑧ 同一口径 ✓）
const twelveEn = await openClientBrokenKernel("en");
const enNotice = twelveEn && twelveEn.notice ? twelveEn.notice : "";
if (!(twelveEn && twelveEn.stubHits > 0)) {
  // 桩在这一次导航里没接到 ⇒ 「退回说明」根本不会出现 ⇒ 直接判产品红是**冤枉** ✗ 记成判据自身失效 ✓。
  judgeBroken.push("⑫ 英文那趟的 fetch 桩没接到 GET /api/atoms（命中 " +
    JSON.stringify(twelveEn && twelveEn.stubHits) + "）⇒ 退回这条路压根没走 ✓ 英文词条那两条不作数");
} else {
if (!/could not load this document/.test(enNotice)) bad("⑫ 英文模式下那句退回说明还是中文（`I18N_EN_TEXT` 缺词条 ✗）", enNotice);
else ok("⑫ 英文模式下退回说明是整句英文 ✓", enNotice);
if (/[\u4e00-\u9fff]/.test(enNotice)) bad("⑫ 英文模式的退回说明里漏出汉字 ⇒ 词条只翻了一半 ✗", enNotice);
else ok("⑫ 英文那句里没有漏出的汉字 ✓（整句翻完了 ✗ 不是半中半英 ✓）");
await capture("08-kernel-fallback-en");
}

// 桩**用完必须摘掉** ✗：它挂在"每一个新文档"上 ✓ 留着会污染后面 ⑩ 以及任何一次导航 ✓
//   ⇒ 摘这一句要在 `if (twelvePremise)` 那个块**外面** ✓ 前提立不住时桩照样可能已经挂上了 ✗。
if (stubId !== null) await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: stubId });
}

// ---------- ⑩ JS 错误 ----------
if (pageErrors.length > 0) bad("⑩ 页面抛了 JS 错误 ⇒ 预检把落笔路径打断了 ✗", pageErrors.slice(0, 4));
else ok("⑩ 全程 0 条页面 JS 错误 ✓");

ws.close();
console.log("\n  截图目录：" + shotsDir);
if (problems.length > 0) {
  if (judgeBroken.length > 0) {
    for (const line of judgeBroken) console.error("  （另有判据自身没立住的前提：" + line + "）");
  }
  console.log(`  ✗ ${problems.length} 处不符（锁定图层的 Web 反馈）`);
  process.exit(1);
}
if (judgeBroken.length > 0) {
  for (const line of judgeBroken) console.error("❌ 判据自身失效：" + line);
  console.error("  ⇒ 有 " + judgeBroken.length + " 条断言没测到东西 ⇒ 这次不算绿（退出码 3 ⇒ 与产品红区分开 ✓）");
  process.exit(3);
}
console.log("  ✓ 锁定图层上落笔：拦在开始之前 · 话说在看得见的地方 · 合法的不误伤 · 解锁后照画 · 双语齐 · 另一头锁的也会自己认账");
process.exit(0);
