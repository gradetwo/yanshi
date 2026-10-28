#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
/// **参考图叠加层：解不开的那一半必须真的看不见** ✓（第四轮补 ✓）。
///
/// 用法: node scripts/browser-reference-decode.mjs <viewer-url> [cdp-port]
///   退出码：0 = 通过 ✓ 1 = 产品不符 ✓ 2 = 用法错 ✓ 3 = **判据自身前提不成立**（不是产品结论 ✓）。
///
/// ## 与 `browser-reference-overlay.mjs` 的分工 ✓
/// 那一条用 `render_region` 产 blob ⇒ 存储里是**真 PNG** ⇒ `<img>` 永远解得开 ✓
/// ⇒ 它测的是"**设了就该看得见**" ✓ 而**看不见的那条路它走不到** ✗。
/// 本条专门走**另一条路**：`POST /api/blob` 把 PNG **解码成裸 RGBA** 入库 ✓（Agent 用 MCP 设参考图
/// 就是这种 ✓）⇒ 浏览器解不开 ⇒ 测的是"**解不开时降级得干不干净**" ✓（破图图标 / 中文 alt / 说明句 /
/// 重新上传能否恢复 / 外部清除后是否收起 ✓）。**两条互补 ✓ 合起来才覆盖参考图叠加的两侧 ✓**。
///
/// ## 这一条测的是什么缺陷 ✗
/// 参考图**只记在偏好里** ✓（`set_reference` 的契约 ✓ 文档逐字节不动 ✓）⇒ 查看器在画布上
/// **叠一层 `<img>`** ✓。而存储里那份字节**常常是裸 RGBA** ✓（`POST /api/blob` 把 PNG 解码后入库 ✓
/// 实测 `GET /api/blob/…` 回 `Content-Type: application/octet-stream` ✓ 16 字节 ✓）⇒
/// 本浏览器**没缓存过原图**时（Agent 用 MCP 设的参考图就是这种 ✓）`<img>` 解不开 ✗
/// ⇒ 浏览器在**画面正中**画一个**破图图标 + 中文 alt** ✓（真机截图里看得见 ✓）。
/// 卡片那句「本浏览器没有它的原图」是对的 ✓ 但画布上留了个坏掉的占位符 ✗ ⇒
/// **"降级"做成了"脏东西"** ✓ 本轮修的就是这一半 ✓。
///
/// ## 判据为什么必须读页面而不是读接口 ✓
/// 解码成不成功是**浏览器**的事 ✗ 服务端回执里永远是 `ok: true` ✓
/// ⇒ 只有 CDP 里那个 `<img>` 的 `error`/`load` 与它的 `display` 才是用户看到的东西 ✓。
///
/// ## 口径 ✓
/// - **偏好是全局的**（实测：`<root>/preferences.json` 一个文件 ✓ 302 个文档共用 ✓）
///   ⇒ 本判据**结束前必须 `clear_reference`** ✓ 否则破图会留在别人的截图里 ✓（本轮就是这么发现的 ✓）。
/// - 先清 `localStorage` 里的 `yanshi.reference.*` ✓ 否则本机缓存会让 `<img>` 解得开 ⇒ 判据空转 ✓。

// **★ 总超时守卫 ✗ ★**（本仓约定 ✓ 与 `browser-reference-overlay.mjs` 同族 ✓）：本判据要连 CDP 浏览器
//   ⇒ 浏览器没起来时它可能**一直等** ✗（把 CI 的判据分片拖到跑不完 ✓）。外层 `run-criteria.sh` 给
//   browser-* 的是 `timeout 240` ✓ ⇒ **内层必须落在它之下** ✓（否则外层先砍 ⇒ 124 ⇒ 红绿不分 ✗）。
//   **超时还要清偏好** ✓：参考图记在**全局一个文件**里 ✓ 半途挂掉会把「解不开的参考图」
//   留给后面**所有**判据的截图 ✗（本轮这个缺陷就是这么被发现的 ✓）。
let fixture = null;
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  const sweep = fixture
    ? fetch(`${fixture.base}/api/tools/clear_reference?doc=${encodeURIComponent(fixture.doc)}&token=${encodeURIComponent(fixture.token)}`,
            { method: "POST", headers: { "content-type": "application/json" }, body: "{}" }).catch(() => {})
    : Promise.resolve();
  Promise.race([sweep, sleep(1500)]).finally(() => process.exit(3));
}, 180000);

const fs = await import("node:fs/promises");
const zlib = await import("node:zlib");

const url = process.argv[2];
// **★ 端口优先读环境变量 ✗ ★**（`browser-reference-overlay.mjs:20-25` 同一处教训 ✓）：
//   `run-criteria.sh` 给 browser-* 传 `<viewer-url> <server-base> <token> <cdpPort>` ✓
//   ⇒ argv[3] 是个 URL ✗ 拿它当端口 ⇒ 拼出坏 URL ⇒ CI 里**恒红**（本轮自查发现 ✓）。
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
if (!url) {
  console.error("用法: node scripts/browser-reference-decode.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const base = new URL(url).origin;
const shotsDir = process.env.SHOTS_DIR || "/tmp/yanshi-reference-overlay-shots";
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};
const sleep = (ms) => new Promise((res) => setTimeout(res, ms));

// ---------- 夹具：一张**真 PNG** 走 `put_blob` ⇒ 存储里就是裸像素 ✓ ----------
function tinyPng(w, h, rgba) {
  const crcTable = [];
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
  }
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, body) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(body.length);
    const t = Buffer.from(type, "ascii");
    const c = Buffer.alloc(4); c.writeUInt32BE(crc(Buffer.concat([t, body])));
    return Buffer.concat([len, t, body, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  const raw = Buffer.alloc(h * (1 + w * 4));
  for (let y = 0; y < h; y += 1) {
    const row = y * (1 + w * 4);
    raw[row] = 0;
    for (let x = 0; x < w; x += 1) {
      const o = row + 1 + x * 4;
      raw[o] = rgba[0]; raw[o + 1] = rgba[1]; raw[o + 2] = rgba[2]; raw[o + 3] = rgba[3];
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const docId = "t_refoverlay_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 160, height: 120,
                         background: { r: 255, g: 255, b: 255, a: 255 } }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
fixture = { base, doc: docId, token };
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());

const png = tinyPng(8, 8, [220, 40, 40, 255]);
// **夹具走的是查看器自己那条上传路** ✓（`POST /api/blob` ✓ 不是 `put_blob` ✓）：
// 实测 `POST /api/blob` 把 PNG **解码成裸 RGBA 再入库** ✓（8×8 ⇒ `size:256` ✓
// `mime_type: image/x-yanshi-raw` ✓）⇒ `GET /api/blob/…` 回 `application/octet-stream` ✗
// ⇒ `<img>` 解不开 ✓ 这才是破图的那条路 ✓。
// （`put_blob` 存的是**原字节** ✓ 直接给 `<img>` 解得开 ✓ —— 第一版夹具误用了它 ⇒
//   判据在"解得开"的路径上空转 ✗ 本轮改成上面这条 ✓。）
const put = await fetch(`${base}/api/blob?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "image/png" }, body: png }).then((r) => r.json());
const blobHash = put.blob_hash;
if (!put.ok || !blobHash) { console.error("❌ 判据前提失败：POST /api/blob ⇒ " + JSON.stringify(put).slice(0, 200)); process.exit(3); }
if (!(put.decoded_from && put.decoded_from.format === "png")) {
  console.error("❌ 判据前提失败：上传没有走解码入库（`decoded_from` = " + JSON.stringify(put.decoded_from) + "）⇒ 破图路径不复现");
  process.exit(3);
}
// **实测这一步** ✓：取回来的到底是什么（判据不能假设产品行为 ✗）。
// **URL 里那个冒号不许被编码掉** ✓：`sha256:…` 走 `encodeURIComponent` ⇒ `%3A` ⇒
//   服务端按"路径里没这个哈希"回一段 **JSON 错误**（实测 102 字节 `application/json` ✓）⇒
//   而"不是 image/png"看起来正好等于"解不开" ✓ 判据就**为错误的原因打了绿勾** ✗
//   ——那比不判还坏 ✓ 所以这里既比 content-type **也比字节数** ✓（裸 RGBA ⇒ 必为 `w*h*4` ✓）。
const blobUrl = `${base}/api/blob/${blobHash}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`;
const blobHead = await fetch(blobUrl);
const blobBytes = Buffer.from(await blobHead.arrayBuffer());
const blobType = blobHead.headers.get("content-type") || "";
const decodable = /^(image\/png|image\/jpeg|image\/webp|image\/gif)/.test(blobType);
ok(`前提：参考图字节服务端回 ${blobType} · ${blobBytes.length} 字节（` +
   `${decodable ? "可解码 ⇒ 本判据的破图那一半测不到 ✗" : "解不开 ⇒ 正是破图那条路 ✓"}）`);
if (!blobType.includes("octet-stream") || blobBytes.length !== 8 * 8 * 4) {
  console.error("❌ 判据前提失败：`GET /api/blob/…` 没回 256 字节裸像素（实为 " +
                JSON.stringify(blobType) + " · " + blobBytes.length + " 字节 · 正文 " +
                blobBytes.subarray(0, 60).toString("utf8") + "）⇒ 破图路径没有真的复现");
  await callTool("clear_reference", {});
  process.exit(3);
}
const setRef = await callTool("set_reference", { blob_hash: blobHash, opacity: 0.5,
                                                 position: { x: 20, y: 15, w: 80, h: 50 } });
if (!setRef.ok) { console.error("❌ 判据前提失败：set_reference ⇒ " + JSON.stringify(setRef).slice(0, 200)); process.exit(3); }

// ---------- CDP ----------
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  await callTool("clear_reference", {});   // 偏好已设 ⇒ 不清会把破图留给后面所有判据 ✓
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) {
  console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）");
  await callTool("clear_reference", {});
  process.exit(3);
}
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
});
await new Promise((res, rej) => { ws.addEventListener("open", res); ws.addEventListener("error", rej); });
const send = (method, params = {}) => new Promise((res) => {
  const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params }));
});
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
/** `Runtime.evaluate` 必须 `returnByValue` + `awaitPromise` 成对 ✓（AGENTS.md ✓）。 */
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}
const capture = async (name) => {
  // **截图前切到「素材」tab 并把参考图卡滚到眼前** ✓（这张卡 `data-panel="assets"` ✓
  //   默认显示的是「绘制」✓ ⇒ 不切 tab 就永远拍不到它 ✓ 那"真实截图检查"就是空的 ✓）。
  await evaluate(`(() => {
    const tab = document.querySelector('#rightTabs button[data-tab="assets"]');
    if (tab) tab.click();
    return 1;
  })()`);
  await sleep(200);
  // **滚到这张卡并**确认到位**（重试到布局稳定 ✓）**：页面在开机／`resync` 之后会重排右侧面板 ✓
  //   一次 `scrollTop` 会被抹掉 ✗（实测：英文模式那张截图拍到的是「对象」卡 ✓）。
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const landed = await evaluate(`(() => {
      const card = document.getElementById("cardReference");
      if (!card) return false;
      const pane = card.closest("aside") || card.parentElement;
      const cr = card.getBoundingClientRect(), pr = pane.getBoundingClientRect();
      if (cr.top >= pr.top - 2 && cr.top < pr.bottom) return true;
      pane.scrollTop += (cr.top - pr.top) - 8;
      return false;
    })()`);
    if (landed) break;
    await new Promise((res) => setTimeout(res, 150));
  }
  await new Promise((res) => setTimeout(res, 200));
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  await fs.mkdir(shotsDir, { recursive: true });
  const file = `${shotsDir}/${name}.png`;
  await fs.writeFile(file, Buffer.from(data, "base64"));
  return file;
};

await send("Runtime.enable");
await send("Page.enable");
// **把窗口撑到常见桌面尺寸** ✓（默认 800×600 时右侧面板只有 ~84px 高 ✓ 卡片根本拍不全 ✓
//   ⇒ "真实截图检查"会拍成一排按钮 ✓ 看不出界面好坏 ✓）。
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
await send("Log.enable");
// 进文档前先清**本机参考图缓存** ✓（`yanshi.reference.<hash>` ✓）：
// 留着它 ⇒ `<img>` 拿 dataURL ✓ 解得开 ✓ ⇒ 破图那条路根本没跑 ✓ 判据就成了空转 ✗。
// **★ 导航 URL 上钉死 `render=server` ✗ ★**（本轮 CI 接线自查发现 ✓）：
//   `run-criteria.sh` 给 browser-* 一族**统一加 `&render=client`** ✓ 而 `viewer-app.js:83-88` 里
//   **URL 参数优先级在 `localStorage` 之上** ✗ ⇒ 光擦存储擦不掉它 ✓ 页面照样走本地内核 ✓
//   ⇒ 本判据**经服务端 API 写下去的状态**（图层/原子/偏好）在页面上**读不回来** ✓
//   测的就不再是它要测的那一半 ✓（本机手工跑不带这个参数 ⇒ 看不出来 ✗ CI 里恒红 ✓）。
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(700);
await evaluate(`(() => {
  const keys = [];
  for (let i = 0; i < localStorage.length; i += 1) {
    const k = localStorage.key(i);
    if (k && k.startsWith("yanshi.reference.")) keys.push(k);
  }
  keys.forEach((k) => localStorage.removeItem(k));
  localStorage.removeItem("yanshi.serverRender");
  localStorage.removeItem("yanshi.lang");
  return keys.length;
})()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
// **前提：页面确实在「服务器渲染」那一侧** ✓（与 `browser-fill-region`／`browser-blend-modes` 同款 ✓）：
//   本判据的参考图是**经服务端 API** 写进偏好的 ✓ 而偏好有**两份存储**（服务端 `preferences.json` ✓
//   与本地内核的 IndexedDB `prefs` ✓）⇒ 页面若跑在本地模式就读不到 ⇒ 后面每条断言都在判一个**空场景** ✗
//   ⇒ 与其判错对象 ⇒ 不如报「前提不成立」退 3 ✓。
//   **★ 必须有界地等 ✗ ★**（本轮实测踩到 ✓）：`loadEventFired` 可能来自**上一次**导航 ✓
//   而勾选框的 `checked` 是脚本执行时才赋值的 ✓ ⇒ 只读一次会读到**还没换页的旧文档** ✓
//   （旧页是别的判据留下的 `render=client` ⇒ 恒 false ⇒ 假红 ✗）⇒ 这里等到**真状态** ✓ 或超时 ✓。
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
  console.error("❌ 判据前提失败：页面没落到「服务器渲染」那一侧（实为 " + JSON.stringify(serverRenderOn) + "）⇒ 读不到服务端偏好，后面读到的都是空场景");
  await callTool("clear_reference", {});
  process.exit(3);
}

const readOverlay = () => evaluate(`(() => {
  const el = document.getElementById("referenceOverlay");
  if (!el) return JSON.stringify({ present: false });
  const style = getComputedStyle(el);
  const box = el.getBoundingClientRect();
  const info = document.getElementById("referenceInfo");
  return JSON.stringify({
    present: true,
    decode: el.dataset.decode || "",
    source: el.dataset.source || "",
    display: style.display,
    visibility: style.visibility,
    opacity: style.opacity,
    alt: el.alt,
    box: { w: Math.round(box.width), h: Math.round(box.height) },
    info: info ? info.textContent : null,
    blob: (el.dataset.blob || "").slice(0, 18),
  });
})()`).then((s) => JSON.parse(s));

let overlay = null;
for (let i = 0; i < 80; i += 1) {
  overlay = await readOverlay();
  if (overlay.present && overlay.decode) break;
  await sleep(250);
}
if (!overlay.present) {
  console.error("❌ 判据前提失败：偏好里有参考图，页面却没建叠加层（`refreshReferenceOverlay` 没跑？）");
  await callTool("clear_reference", {});
  process.exit(3);
}
if (!overlay.decode) {
  console.error("❌ 判据前提失败：等了 20 秒 `<img>` 既没 load 也没 error ⇒ 这一条没测到东西");
  await callTool("clear_reference", {});
  process.exit(3);
}

// ---------- ① 解不开 ⇒ 这一层必须真的看不见 ----------
if (overlay.decode === "failed" && overlay.display === "none") {
  ok(`① 裸像素解不开 ⇒ 叠加层收起（display=none ✓ 尺寸 ${JSON.stringify(overlay.box)}）`, { decode: overlay.decode });
} else if (overlay.decode === "ok") {
  bad(`① 这条路径本应解不开却报 load 成功 ⇒ 判据夹具失效`, overlay);
} else {
  bad(`① 解不开却仍留在画布上（display=${JSON.stringify(overlay.display)}）⇒ 用户看到的就是那个破图图标`, overlay);
}
// ---------- ② 收起之后要把话说完（不许静默消失）----------
if (overlay.info && /裸像素/.test(overlay.info) && /收起/.test(overlay.info)) {
  ok(`② 卡片讲清了为什么看不见`, { info: overlay.info });
} else {
  bad(`② 叠加层收起了但界面没解释 ⇒「参考图明明设了却看不见」又变回一个谜`, { info: overlay.info });
}
await capture("zh-01-collapsed");

// ---------- ③ alt 是要翻的界面文案（图解不开时它就是画布上唯一可见的字）----------
if (overlay.alt === "参考图") ok(`③ 中文模式下 alt 保持中文 ✓`, { alt: overlay.alt });
else bad(`③ 中文模式下 alt 被改写了 ⇒ 回切逻辑坏了`, { alt: overlay.alt });

// ---------- ④ 英文模式：0 中文残留 ----------
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=en&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let en = null;
for (let i = 0; i < 80; i += 1) {
  en = await readOverlay();
  if (en.present && en.decode && en.info) break;
  await sleep(250);
}
if (!en.present || !en.decode) {
  console.error("❌ 判据前提失败：英文模式下叠加层没建起来 / 没给出解码结果");
  await callTool("clear_reference", {});
  process.exit(3);
}
const cjk = /[㐀-鿿]/;
if (en.display === "none") ok(`④ 英文模式下同样收起（display=none ✓）`);
else bad(`④ 英文模式下破图又留在画布上了`, en);
if (!cjk.test(String(en.info || "")) && /raw pixels/.test(String(en.info || ""))) {
  ok(`④ 英文模式下那句解释是英文 ✓`, { info: en.info });
} else {
  bad(`④ 英文模式下 ` + `#referenceInfo` + ` 漏中文 / 没翻`, { info: en.info });
}
if (!cjk.test(String(en.alt || ""))) ok(`④ alt 跟着翻了 ✓`, { alt: en.alt });
else bad(`④ 英文模式下 alt 还是中文 ⇒ 画布上那个占位符就是中文`, { alt: en.alt });
await capture("en-04-collapsed-english");

// ---------- ⑤ 重新上传（本会话有原图）⇒ 必须真的看得见 ----------
// **图在页面里用 canvas 现做** ✓（不复用上面那个手写 PNG ✗）：手写编码器**服务端解得开** ✓
//   而 chromium 的 `createImageBitmap` **不认** ✓（实测 `InvalidStateError: The source image could not be decoded` ✓）
//   ⇒ 判据不该拿自家编码器的兼容性去测产品 ✓ canvas 出来的 PNG 一定是浏览器认的 ✓。
const uploaded = await evaluate(`(async () => {
  const input = document.getElementById("referenceFile");
  if (!input) return "NO-INPUT";
  const c = document.createElement("canvas");
  c.width = 8; c.height = 8;
  const g = c.getContext("2d");
  g.fillStyle = "rgb(220, 40, 40)";
  g.fillRect(0, 0, 8, 8);
  const blob = await new Promise((res) => c.toBlob(res, "image/png"));
  if (!blob) return "NO-BLOB";
  const file = new File([blob], "ref.png", { type: "image/png" });
  const dt = new DataTransfer();
  dt.items.add(file);
  input.files = dt.files;
  input.dispatchEvent(new Event("change", { bubbles: true }));
  return "DISPATCHED";
})()`);
let visible = null;
for (let i = 0; i < 80; i += 1) {
  visible = await readOverlay();
  if (visible.decode === "ok" && visible.display !== "none") break;
  await sleep(250);
}
const logTail = await evaluate(`(() => {
  const box = document.getElementById("log");
  if (!box) return [];
  return Array.from(box.children).map((n) => String(n.textContent).slice(0, 90)).filter(Boolean).slice(-6);
})()`);
if (visible.decode === "ok" && visible.display !== "none" && visible.box.w > 4 && visible.box.h > 4) {
  ok(`⑤ 本会话上传过 ⇒ 参考图真的显示出来（${JSON.stringify(visible.box)} · 来源 ${visible.source}）`, { uploaded });
} else {
  bad(`⑤ 重新上传后仍然看不见 ⇒「上传一次就能一直显示」那句是假的`, { uploaded, read: visible, logTail });
}
await capture("en-05-visible-after-upload");

// ---------- ⑥ 外部（Agent）清掉参考图 ⇒ 页面下一次刷新必须收起这一层 ----------
// 偏好改动**不产生原子** ⇒ 服务端没有 head 变化也不广播 ✗ ⇒ 页面只能在自己刷新时重读 ✓
//（`clearReferenceImage` 走的就是 `resize` 监听这条路 ✓ 判据用同一条 ✓）。
const cleared = await callTool("clear_reference", {});
await evaluate(`(() => { window.dispatchEvent(new Event("resize")); return 1; })()`);
let gone = false;
for (let i = 0; i < 60; i += 1) {
  gone = !(await evaluate(`(() => !!document.getElementById("referenceOverlay"))()`));
  if (gone) break;
  await sleep(250);
}
if (cleared.ok && gone) ok(`⑥ clear_reference 之后叠加层从 DOM 移除 ✓（不留残影 ✓）`);
else bad(`⑥ 清掉参考图后画面上还有东西`, { cleared: { ok: cleared.ok }, gone });

if (pageErrors.length === 0) ok(`⑦ 全程 0 条页面 JS 错误`);
else bad(`⑦ 页面抛了 ${pageErrors.length} 条 JS 错误`, pageErrors.slice(0, 4));

ws.close();
console.log("  截图目录：" + shotsDir);
if (problems.length > 0) {
  console.log(`  ✗ ${problems.length} 处不符（参考图叠加层的降级）`);
  process.exit(1);
}
console.log("  ✓ 参考图解不开时收起得干净、说得清、换语言不残留、重新上传看得见");
process.exit(0);
