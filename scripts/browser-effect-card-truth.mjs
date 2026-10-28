#!/usr/bin/env node
// criterion-target: server  ## 运行器据此决定是否注入 render=client（主体测服务端 add/update/delete + render_region 指纹；本地臂自己走「URL 不带 render」）
// **判据：调整/滤镜卡（`#cardEffects`）在卡上写下的每一句结论，都必须能被服务端同一时刻的事实复现** ✓。
//
// **抓的是哪一类缺口** ✓：这张卡过去的回执是「已应用 X」——**一句界面自己下的结论** ✗
// 本轮实测它不可信（`/tmp/yanshi-web-r7/probe-noop.mjs` 把 26 个调整逐个建→量→删 ✓ 全绿可复现）：
//   ① 26 个调整里 **17 个在缺省参数 `{}` 下整幅合成逐字节不变**（`calibrate_primaries` 就是其一 ✓）
//      ✓ 7 个默认就动画面 ✓ 另 2 个（`color_lut`／`perspective`）**连 `{}` 都建不出来** ⇒ 服务端直接拒 ✗
//      ⇒ 同一句「已应用」底下两种事实长得一模一样 ✗（写文档时把这三个数加总回 26 才发现原先
//      把"被拒的两个"算进了空操作 ⇒ 17+7+2=26 ✓ 名单与数字必须自洽 ✓）；
//   ② **参数名拼错服务端照单收下**（`{"redhue":50}` ⇒ `ok:true` ✓ 原样存着 ✓ 画面一字不动 ✗）
//      —— 内核只校验它**认识**的键 ✓ 于是"成功"与"没效果"之间的落差全被那句回执吞掉 ✓；
//   ③ 效果**只进不出** ✗ 空操作叠五次也撤不掉 ✓（`delete_object` 实测能删效果对象并复原画面 ✓）。
// ⇒ 修复把"动没动"交给**服务端整幅合成的指纹**（`render_region` 只读 ✓ 不产生原子 ✓）与
//   `sample_colors` 的合成读数 ✓ 取样窗口取自 `list_objects` 的对象外框 ✓。
// **本条判据因此不读文案通顺不通顺** ✗ 它每一件都要**自己独立再测一遍** ✓
// 页面说的话与服务端的事实**不许有两个真值** ✓（与减淡加深那条同一口径 ✓）。
//
// **钉住的十四件事**（⓪~⑩ 加 ③′、③″ 与 ⑩d ✓）：
//   ⓪ 卡在位 ✓ 折叠时点标题能展开 ✓ 展开后 `#effectInfo` 与「应用」按钮**真的可见** ✓
//      （写在 `display:none` 里的结论对界面用户等于没说 ✓）；效果名列表来自服务端目录 ✓
//      判据只要求本轮要用的 `calibrate_primaries` 与 `invert` 在场 ✗ 不写死总数 ✓；
//   ① 真参数（`red_hue:80`）⇒ 页面说「画面确实动了」**同时**判据自己直调 `render_region`
//      也测到哈希变了 ✓（谁把结论硬编码成"没变"当场红 ✓）；
//   ② 页面报的那个「实测 (x,y) = [前] → [后]」**逐位对得上判据的独立读数** ✓：
//      坐标必须落在**内容外框**内（夹具把 40×40 的红放在 (540,340) ⇒ 文档中心是白的 ✓
//      谁退回均匀网格就取不到这个坐标 ✓ 当场红 ✗）、前值等于**应用前**判据在同一点的读数 ✓
//      后值等于**应用后**判据再读一次的读数 ✓（界面不许自己算色 ✗）；
//   ③ 拼错参数名 ⇒ 页面必须说「逐字节没变 ⇒ 空操作」✓ 而对象照样存着（回读原样 ✓）
//      ⇒ "服务端收了"与"画面动了"是两件事 ✓ 卡上必须分开说 ✓；
//   ③′ **那句「空操作」说明只在它成立时挂在屏幕上** ✓（★ 第七轮看真截图才补的 ✗
//      原先它一直挂着 ⇒「画面确实动了」与「未写入」底下都跟着它 ⇒ 三种状态两种说假话 ✓
//      只降亮度挡不住 ✗）⇒ 在 ①′（动了）③′（空操作）⑥′（被拒）⑦′（没出门）四个状态各问一次
//      **可见性** ✓ 不问文案 ✓（"该说的时候才说"是一条屏幕事实 ✗ 不是措辞问题 ✓）；
//   ③″ **屏幕上那句必须短 ✓ 而那两条「为什么」必须还在**（★ 同轮 `ui-text-density` 红出来的 ✓ 见 docs ㊴）：
//      模板级密度判据只读 `viewer.rs` ✗ 看不见运行时改写的可见文本 ✓ 更看不见"为了过判据直接删掉" ✗
//      ⇒ 这里在**渲染后的元素上**两头都问：可见文本 < 24 个汉字 ✓ 而 `title` 里那句原因（含
//      「内核只校验它认识的键」✓ 中英两种都算 ✓）必须还挂着 ⇒ **搬家 ≠ 失忆** ✓；
//   ④ 删掉那个空操作 ⇒ 页面说「它原本就没参与画面」✓ 删掉真效果 ⇒ 页面说「动了」且
//      **指纹回到判据记下的基线** ✓ 内容处读数复原 ✓（"删了"两个字不算交代 ✗）；
//   ⑤ 缺省参数就动的 `invert {}` ⇒ 页面必须说「动了」✓（挡住"一律报空操作"那种反向偷懒 ✗）；
//   ⑥ 越界的**真**参数（`red_hue:150`）⇒ 页面说「未写入：服务端拒绝了这一步（错误码 invalid_argument…）」
//      ✓ 原子数、效果数、指纹**一个都没动** ✓ 而服务端的逐字中文 `detail` **只在 `#log`** ✓
//      卡上不复述（免得英文界面夹整句中文 ✓）；
//   ⑦ 参数框非法 JSON ⇒ 卡上说「没有发送请求、没有写入任何改动」✓ 而判据核对：原子数、
//      效果数、指纹确实一根汗毛都没动 ✓（先拦后送 ✗ 这话必须写在卡上而不只是日志 ✓）；
//   ⑧ 点一行进入"编辑它"⇒ 按钮文案换「更新这个」✓ 参数框自动装着**它现在的参数** ✓
//      更新后说「已更新」✓ **效果数一个没多**（更新不是新建 ✓）✓ 回读参数是新值 ✓；
//   ⑨ 英文模式 ⇒ 回执行**整句翻译且零汉字** ✓「删」按钮与行 tooltip 零汉字 ✓
//      数字（坐标、RGBA、效果数）仍是服务端那一个 ✓；全程 0 条 JS 错误 ✓。
//   ⑩ 关掉「服务器渲染」⇒ 这条路拿不到服务端指纹 ✓ 页面**不许**宣称"动了/没动"任何一种结论 ✗
//      只能说「没法验证」或「本地内核没有这个端点」✓（本机内核不可用时这条如实标注为跳过 ✗ 不假装测过 ✓）；
//   ⑩b **失败的责任要说给这一笔实际走过的那条路**（本轮判据第一次跑红抓到的真 bug ✗）：
//      关掉服务端渲染后请求落在**本地内核**上 ✓ 而卡上原来把**所有**失败都写成
//      "**服务端**拒绝了这一步" ⇒ 读的人被送去翻服务端日志 ✓ 该看的却在本地内核 ✗。
//      这一臂点名禁止「服务端拒绝」四字 ✓ 而 ⑥ 继续钉住它 ⇒ 两臂合起来才把"话跟着路走"钉死 ✓
//      （单看任何一臂都抓不到 ✓ 只比对"页面 == 回执"也抓不到 ✓ 两边一起说服务端 ✓）；
//      同族还有 **⑩c**：码的格子在服务端叫 `error_code` ✓ 在本地叫 `error` ✓ 卡片原先只读前者
//      ⇒ 本地那一臂念成「错误码 unknown」✗ 而日志里（页面**通用**失败分支写的 ✓ 另一条代码路径 ✓）
//      明明写着具体的码 ⇒ 判据拿日志那个码与卡上那句比 ✓ 谁漏读谁红 ✓。
//      同族第三条 **⑩d**（★ 第二次看真截图抓到的 ✗ 与 ⑩b 只差"另一格"）：回执行说实话了 ✓ 而
//      **清单那一格**只念「读取失败：endpoint_not_local」⇒ 一个裸码 ✗ 同一张卡上一句说责任一句不说 ✓
//      人只能猜是"服务端读不到"还是"本机没这端点" ⇒ 现在那一句也走同一对 `refusalVenue()`／
//      `failureCode()` ✓ 断言钉"带 venue 且带具体码"✓ 而那一格若念出清单或空着 ⇒ **判据前提变了**
//      ⇒ 按本文件口径 `exit(3)` ✓ 既不判成产品红也不判成绿 ✓（静态截图看不到"这一格会不会变红"✓
//      只有真去看那张图才会发现它一直在说半句话 ✓）。
//
// **口径**：导航 URL 钉死 `render=server` ✓（本条测的是服务端工具 ✗ 已加进 `run-criteria.sh`
// 的例外名单 ✓）；每个场景用完就用卡上的「删」清干净 ⇒ 断言互不干扰 ✓；
// 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)` ✓（AGENTS.md：不许把环境问题判成产品红 ✓）。
//
// 用法：node scripts/browser-effect-card-truth.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-effect-card-truth.mjs http://127.0.0.1:15900 9490
import { setTimeout as sleep } from "node:timers/promises";

const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 240000);

const url = process.argv[2];
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
const base = url ? new URL(url).origin : "http://127.0.0.1:15900";
if (!url) {
  console.error("用法: node scripts/browser-effect-card-truth.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};
const skip = (label, detail) => console.log("· 跳过（环境前提）：" + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));

// ---------- 一次性文档 + 夹具 ----------
// **内容故意不在文档中心** ✓（1200×800 的文档里只放一块 40×40 的红在 (540,340) ✓）：
// 均匀网格取样会全落在白底上 ⇒ "画面没动"的谎就这么来的 ✓ 断言 ② 专门钉它 ✗。
const DOC_W = 1200;
const DOC_H = 800;
const RECT = { x: 540, y: 340, w: 40, h: 40 };
const REGION = [RECT.x, RECT.y, RECT.w, RECT.h];
const SPOTS = [[560, 360], [550, 350], [570, 370]];
const docId = "t_eff_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: DOC_W, height: DOC_H }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());
const ART = "ART";
const madeLayer = await callTool("create_layer", { layer_id: ART, name: "内容" });
if (!madeLayer.ok) { console.error("❌ 判据前提失败：建图层 ⇒ " + JSON.stringify(madeLayer).slice(0, 200)); process.exit(3); }
const filled = await callTool("fill_region", {
  layer_id: ART,
  shape: { type: "rect", x: RECT.x, y: RECT.y, w: RECT.w, h: RECT.h },
  color: { r: 200, g: 50, b: 50, a: 255 },
});
if (!filled.ok) { console.error("❌ 判据前提失败：铺内容 ⇒ " + JSON.stringify(filled).slice(0, 200)); process.exit(3); }

// ---------- 判据自己的服务端仪器（界面说的话一律拿它们复现 ✓）----------
const atoms = async () => Number((await callTool("get_log", {})).count || 0);
const effects = async () => (((await callTool("list_effects", {})).effects) || []);
const fingerprint = async () => (await callTool("render_region", { region: REGION })).blob_hash;
const readAt = async (x, y) => {
  const value = await callTool("sample_colors", { points: [[x, y]] });
  const sample = (value.samples || [])[0];
  return sample && Array.isArray(sample.rgba) ? sample.rgba.join(",") : null;
};
const listedBbox = async () => {
  const value = await callTool("list_objects", {});
  for (const item of value.objects || []) {
    const b = item.bbox;
    if (Array.isArray(b) && b.length === 4 && b[2] > 0 && b[3] > 0) return b;
  }
  return null;
};

const baselineHash = await fingerprint();
const baselineAtoms = await atoms();
const contentPixel = await readAt(560, 360);
const centerPixel = await readAt(Math.floor(DOC_W / 2), Math.floor(DOC_H / 2));
if (typeof baselineHash !== "string" || !baselineHash.startsWith("sha256:")) {
  console.error("❌ 判据前提失败：`render_region` 拿不到服务端指纹 ⇒ " + JSON.stringify(baselineHash));
  process.exit(3);
}
if (contentPixel !== "200,50,50,255" || centerPixel !== "255,255,255,255") {
  console.error("❌ 判据前提失败：夹具读数不符（内容 " + contentPixel + "｜中心 " + centerPixel + "）");
  process.exit(3);
}
const box = await listedBbox();
if (!box || JSON.stringify(box) !== JSON.stringify(REGION)) {
  console.error("❌ 判据前提失败：`list_objects` 报的内容外框不是夹具那一块 ⇒ " + JSON.stringify(box));
  process.exit(3);
}
ok("夹具就位：服务端指纹可读、内容在 (540,340) 一块 40×40、文档中心是白的", { box, contentPixel, centerPixel });

// ---------- CDP ----------
const browser = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
if (!browser.webSocketDebuggerUrl) {
  console.error("❌ 判据前提失败：CDP 在 " + port + " 上没给出 webSocketDebuggerUrl（浏览器没起来？）");
  process.exit(3);
}
const ws = new WebSocket(browser.webSocketDebuggerUrl);
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
const sendB = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params })); });
const created = await sendB("Target.createTarget", { url: "about:blank" });
const attached = await sendB("Target.attachToTarget", { targetId: created.result.targetId, flatten: true });
const sessionId = attached.result.sessionId;
const send = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
await send("Runtime.enable");
await send("Page.enable");
await send("Log.enable");
/** `Runtime.evaluate` 必须 `returnByValue` + `awaitPromise` 成对 ✓（AGENTS.md ✓）。 */
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}
/** **擦偏好再进文档** ✓：`localStorage` 按源共享 ✗ 残留 `yanshi.serverRender="0"`
 *  会让判据拿着本地路由的失败当产品结论 ✓。 */
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(500);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); return 1; })()`);
/** **两种导航** ✓：`render:"server"`（默认 ✓ URL 钉死服务端渲染 ✓ 本条主体测的就是服务端工具 ✓）
 *  与 `render:"none"`（**不带那个参数** ⇒ 让「服务器渲染」勾选框说了算 ✓）。
 *  为什么需要后者 ✓：`serverRenderPreferred()` 里 **URL 那一层盖过 `localStorage`** ✓
 *  所以带着 `render=server` 去点勾选框是没用的 ✗（首轮实测：点完 reload 又被 URL 拉回服务端 ✓
 *  于是"本地那一臂"根本没跑到 ⇒ 判据自己走错门 ✓ 不是产品红 ✓）。 */
const gotoLang = async (lang, options = {}) => {
  const render = options.render === "none" ? "" : "&render=server";
  await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=${lang}${render}&v=${Date.now()}` });
  await waitEvent("Page.loadEventFired");
  let booted = false;
  for (let i = 0; i < 80; i += 1) {
    booted = await evaluate(render
      ? `!!(window.yanshi && document.getElementById("effectName") && document.getElementById("effectName").options.length > 5)`
      : `!!(window.yanshi && document.getElementById("useServerRender") && document.getElementById("layer"))`);
    if (booted) break;
    await sleep(250);
  }
  if (!booted) { console.error("❌ 判据前提失败：开机信号没等到（lang=" + lang + " render=" + (render || "none") + "）"); process.exit(3); }
  if (render) {
    let on = null;
    for (let i = 0; i < 80; i += 1) {
      on = await evaluate(`(() => {
        if (!location.search.includes("render=server")) return "WRONG-PAGE";
        const b = document.getElementById("useServerRender");
        return b ? (b.checked === true ? true : false) : null;
      })()`);
      if (on === true) break;
      await sleep(250);
    }
    if (on !== true) { console.error("❌ 判据前提失败：「服务器渲染」勾选没生效（实为 " + JSON.stringify(on) + "）⇒ 本条测的是服务端工具 ✗ 必须走服务端渲染"); process.exit(3); }
  }
  const langNow = await evaluate(`document.documentElement.lang`);
  if (langNow !== (lang === "en" ? "en" : "zh-CN")) {
    console.error("❌ 判据前提失败：语言没落在 " + lang + "（实为 " + JSON.stringify(langNow) + "）");
    process.exit(3);
  }
  // **本地那一臂不按图层前提收口** ✓：关掉服务端渲染后 ✗ 页面用的是**本地内核自己的文档状态**
  // （IndexedDB ✓ 与服务端那一份分叉 ✓）⇒ 那里没有夹具的 `ART` 层 ✓ 切不到它不算产品红 ✗
  // 这一臂只问一件事：**没有服务端指纹的时候，页面有没有装作验证过**✓。
  if (options.needLayer !== false) await useLayer(ART);
};
/** **把当前编辑图层切到夹具那一层** ✓：效果挂在 `state.layerId` 上（与用户切图层同一条路径 ✓）。
 *  判据不切层的话 ✗ 调整就落在"什么内容都没有"的默认层上 ⇒ 整幅合成当然不动 ✓
 *  那测的就不是这张卡的本事了 ✗（首轮跑红正是栽在这儿 ✓ 记下来 ✓）。 */
async function useLayer(layerId) {
  const got = await evaluate(`(() => {
    const sel = document.getElementById("layer");
    if (!sel) return "no-select";
    sel.value = ${JSON.stringify(layerId)};
    if (sel.value !== ${JSON.stringify(layerId)}) return "missing-option";
    sel.dispatchEvent(new Event("change", { bubbles: true }));
    return ((window.yanshi && window.yanshi.state() || {}).layerId) || String(sel.value);
  })()`);
  if (got !== layerId) {
    console.error("❌ 判据前提失败：当前图层切不到 " + layerId + "（实为 " + JSON.stringify(got) + "）⇒ 判据自己没就位");
    process.exit(3);
  }
}
const setField = (id, value) => evaluate(`(() => {
  const el = document.getElementById(${JSON.stringify(id)});
  if (!el) return "no-el";
  el.value = ${JSON.stringify(String(value))};
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  return el.value;
})()`);
/** **展开卡片** ✓：折叠卡的内容在 `display:none` 里 ✗ 判据若只读 `textContent`
 *  就会给一个"用户根本看不见"的结论盖章 ✓ 所以先看默认折叠态、再按真实路径（点标题）展开 ✓。 */
const openCard = () => evaluate(`(() => {
  const card = document.getElementById("cardEffects");
  if (!card) return JSON.stringify({ missing: true });
  const wasCollapsed = card.classList.contains("collapsed");
  if (wasCollapsed) card.querySelector("h2").click();
  const info = document.getElementById("effectInfo");
  const apply = document.getElementById("effectApply");
  const visible = (el) => !!el && el.offsetParent !== null;
  return JSON.stringify({
    wasCollapsed,
    infoInCard: !!(info && card.contains(info)),
    infoVisible: visible(info),
    applyVisible: visible(apply),
    names: info ? info.textContent : null,
  });
})()`).then((s) => JSON.parse(s));
const cardText = () => evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  const t = (id) => (q(id) ? q(id).textContent : null);
  const list = q("effectsList");
  return JSON.stringify({
    info: t("effectInfo"),
    // **清单那一格的整体文本** ✓：读取失败时它不是行列表而是一句话（rows 那一项就空了 ✓）
    // ⇒ 判据要问那句"谁没读到"就必须看见整格文本 ✓（⑩d ✓ 第七轮第二次看真截图补的 ✗）。
    //（注意：这段在模板字符串里 ⇒ **不能出现反引号** ✓ 会把整段截断 ✗ 本轮踩过一次）
    listText: list ? String(list.textContent || "").trim() : null,
    applyLabel: t("effectApply"),
    params: q("effectParams") ? q("effectParams").value : null,
    rows: list ? [...list.children].map((row) => ({
      text: row.textContent,
      title: row.getAttribute("title"),
      obj: ((row.textContent.match(/obj=(\\S+)/) || [])[1] || null),
      del: row.querySelector("button") ? { text: row.querySelector("button").textContent, title: row.querySelector("button").getAttribute("title") } : null,
    })) : [],
    log: t("log"),
    noteHidden: q("effectNote") ? !!q("effectNote").hidden : null,
    noteVisible: q("effectNote") ? (q("effectNote").offsetParent !== null && !q("effectNote").hidden) : null,
    noteText: q("effectNote") ? String(q("effectNote").textContent || "").trim() : null,
    noteTitle: q("effectNote") ? String(q("effectNote").getAttribute("title") || "").trim() : null,
  });
})()`).then((s) => JSON.parse(s));
/** **那句「空操作」说明的可见性必须与此刻的服务端事实同源** ✓（第七轮 ★ 看真截图才发现的缺陷 ✓）：
 *  它原本是**常设**挂在卡上的一句 ⇒ 于是「画面确实动了 · 实测 [200,50,50,255] → [200,143,0,255]」
 *  与「未写入：…拒绝了这一步」底下**都跟着它** ✗ 三种状态里两种在说假话（只降亮度挡不住 ✓）。
 *  这里**不问文案**✓ 只问"这一刻它该不该出现在屏幕上"✓ 也就是把"该说的时候才说"钉成一条可判的屏幕事实 ✓。 */
function checkNote(read, expectVisible, tag) {
  if (read.noteVisible === expectVisible) {
    ok(tag + " 那句「空操作」说明的**可见性**与此刻服务端事实同源（屏幕上" + (expectVisible ? "该出现 ✓ 也出现了" : "不该出现 ✓ 也确实藏着") + "）",
      { noteVisible: read.noteVisible, info: (read.info || "").slice(0, 46) });
  } else {
    bad(tag + " 那句常设说明在屏幕上说错了此刻的事实（" + (expectVisible ? "该出现却藏着" : "不该出现却挂着 ⇒ 把\"动了/没写成\"说成\"空操作\"") + "）",
      { noteVisible: read.noteVisible, noteHidden: read.noteHidden, info: (read.info || "").slice(0, 60) });
  }
}
/** **屏幕上只许剩短句 ✓ 而那两条「为什么」必须还在——搬进 `title=` 而不是被删掉** ✓（第七轮 ★）：
 *  `ui-text-density` 是**模板级**判据（只读 `viewer.rs` 的文本节点 ✗）⇒ 它看不见运行时被改写的可见文本 ✓
 *  也看不见"为了过判据直接把说明删了"这种修法 ✗ 于是这里在**渲染后的那个元素上**两头都问：
 *  可见文本的汉字数必须短 ✓ `title` 里必须还留着那句原因 ⇒ **密度与失忆是两件事** ✓。 */
function checkNoteWhy(read, tag) {
  const cjk = ((read.noteText || "").match(/[\u4e00-\u9fff]/g) || []).length;
  const why = read.noteTitle || "";
  // **两种语言都算**（英文模式下这个 `title` 已被 `I18N_EN_ATTR` 换掉 ✓ 不许因为它翻了就报红）
  const kept = why.includes("内核只校验它认识的键") || why.includes("only validates keys it recognises");
  if (cjk < 24 && kept) {
    ok(tag + " 屏幕上只剩短句（实测 " + cjk + " 个汉字 ✓ 密度合格）而「为什么」还在（`title` " + why.length + " 字 ⇒ 是搬家不是删掉）",
      { noteText: (read.noteText || "").slice(0, 40), cjk, titleLength: why.length });
  } else {
    bad(tag + " 那句说明的密度／去向不对：可见 " + cjk + " 个汉字 ✓ `title` " + (why ? "有 " + why.length + " 字" : "空") + " ⇒ 要么界面又变回大段解释 ✓ 要么为了过判据把原因删了",
      { noteText: (read.noteText || "").slice(0, 60), cjk, title: why.slice(0, 60) });
  }
}
const waitForInfo = async (pattern, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    const read = await cardText();
    if (read.info && (typeof pattern === "string" ? read.info.includes(pattern) : pattern.test(read.info))) return read;
    await sleep(250);
  }
  return cardText();
};
const applyNow = () => evaluate(`(() => { const b = document.getElementById("effectApply"); if (!b) return "no-button"; b.click(); return 1; })()`);
/** **每按一次之前先把结论行清空** ✓：`waitForInfo` 是"看到预期形状才算数" ✓
 *  留着上一轮那句 ✗ 就会秒回一个**上一轮的**结论 ⇒ 判据假绿/假红都可能出现 ✓（首轮实测被旧的
 *  「已新建 …」蒙过一次 ✓）。清空后才等 ⇒ 等到的必然是**这一次**写出来的话 ✓。 */
const clearInfo = () => evaluate(`(() => { const el = document.getElementById("effectInfo"); if (el) el.textContent = ""; return 1; })()`);
/** 用**卡上的行内「删」**清场 ✓（顺带每轮都走一遍删除路径 ✗ 不留"只在断言里存在"的出口 ✓）。 */
const deleteRow = async (objectId) => {
  await clearInfo();
  return evaluate(`(() => {
  const list = document.getElementById("effectsList");
  if (!list) return "no-list";
  for (const row of list.children) {
    if ((row.textContent || "").includes("obj=" + ${JSON.stringify(objectId)})) {
      const button = row.querySelector("button");
      if (!button) return "no-button";
      button.click();
      return 1;
    }
  }
  return "row-missing";
})()`);
};
/** **一个效果属于"调整"还是"滤镜"由服务端目录说了算** ✓（判据不猜类别 ✗
 *  写死了就会红在判据自己头上 ✓）：按目录决定该切哪一档 ✓ 两类都没有 ⇒ 前提失败 ✓。 */
const pickEffect = async (name) => {
  const kinds = [];
  if ((catalog.adjustments || []).includes(name)) kinds.push("adjustment");
  if ((catalog.filters || []).includes(name)) kinds.push("filter");
  if (kinds.length === 0) {
    console.error("❌ 判据前提失败：服务端目录里没有「" + name + "」这一类效果（目录到手了吗？）");
    process.exit(3);
  }
  await setField("effectKind", kinds[0]);
  await sleep(250);
  const got = await setField("effectName", name);
  if (got !== name) {
    console.error("❌ 判据前提失败：切到 " + kinds[0] + " 后下拉里点不到 " + name + "（实为 " + JSON.stringify(got) + "）");
    process.exit(3);
  }
  return kinds[0];
};
const runApply = async (name, params, pattern) => {
  await pickEffect(name);
  await setField("effectParams", params);
  await clearInfo();
  await applyNow();
  return waitForInfo(pattern);
};

await gotoLang("zh");

// ---------- ⓪ 卡在位、能展开、结论写在看得见的地方 ----------
const shell = await openCard();
if (shell.missing) bad("调整/滤镜卡 `#cardEffects` 不在页面上");
else {
  ok("卡在位（默认" + (shell.wasCollapsed ? "折叠" : "展开") + " ⇒ 点标题展开后控件可见）", { wasCollapsed: shell.wasCollapsed });
  if (shell.infoInCard && shell.infoVisible && shell.applyVisible) ok("展开后 `#effectInfo` 与「应用」按钮真的可见（结论不是写在 `display:none` 里）");
  else bad("卡片结论区不可见（折叠状态没被点开，或 `#effectInfo` 不在卡内）", shell);
}
const catalog = await evaluate(`(() => {
  const sel = document.getElementById("effectName");
  const fill = () => [...(sel ? sel.options : [])].map((o) => o.value);
  document.getElementById("effectKind").value = "adjustment";
  document.getElementById("effectKind").dispatchEvent(new Event("change", { bubbles: true }));
  const adjustments = fill();
  document.getElementById("effectKind").value = "filter";
  document.getElementById("effectKind").dispatchEvent(new Event("change", { bubbles: true }));
  const filters = fill();
  document.getElementById("effectKind").value = "adjustment";
  document.getElementById("effectKind").dispatchEvent(new Event("change", { bubbles: true }));
  return JSON.stringify({ adjustments, filters });
})()`).then((s) => JSON.parse(s));
if (catalog.adjustments.includes("calibrate_primaries") && catalog.filters.length > 0) {
  ok("效果名下拉装着服务端目录（本轮要用的 `calibrate_primaries` 在场 ✓ 不写死总数 ✓）", { adjustments: catalog.adjustments.length, filters: catalog.filters.length });
} else {
  bad("下拉里没有服务端目录（界面自己在拼效果名单？）", { adjustments: catalog.adjustments.slice(0, 6) });
}

// ---------- ① + ② 真参数：页面说「动了」，判据自己也得同样结论，且坐标落在内容上 ----------
await setField("effectParams", "{}");
const before1 = { atoms: await atoms(), effects: (await effects()).length, hash: await fingerprint() };
const spotsBefore1 = {};
for (const [x, y] of SPOTS) spotsBefore1[x + "," + y] = await readAt(x, y);
let read = await runApply("calibrate_primaries", '{"red_hue": 80}', "已新建");
const hashAfter1 = await fingerprint();
const after1 = { atoms: await atoms(), effects: (await effects()).length };
const serverMoved1 = hashAfter1 !== before1.hash;
if (serverMoved1) ok("判据独立测量：`red_hue:80` 之后整幅合成指纹确实变了", { before: before1.hash.slice(0, 18), after: hashAfter1.slice(0, 18) });
else bad("判据独立测量：服务端指纹没变（夹具或内核变了 ⇒ ① 的口径要重测）", { before: before1.hash, after: hashAfter1 });
if (read.info && /⇒\s*整幅合成的服务端指纹变了 ⇒ 画面确实动了/.test(read.info) && serverMoved1) {
  ok("① 页面念「画面确实动了」而且服务端指纹同步变了（结论不是硬编码 ✗ 是比出来的 ✓）", { page: read.info.slice(0, 60) });
  checkNote(read, false, "①′");
} else {
  bad("① 页面那句结论与服务端指纹不同源", { page: read.info, serverMoved: serverMoved1 });
}
const paramsEcho = JSON.stringify((await effects()).map((e) => e.params).filter((p) => p && p.red_hue === 80));
if (read.info && read.info.includes('（参数 {"red_hue":80}）')) ok("② 参数是从回读里念出来的（服务端那份 `{\"red_hue\":80}` ✓ 不是输入框原样 ✓）", { echo: paramsEcho });
else bad("② 回执行没把服务端存的参数念回来", { page: read.info, server: paramsEcho });
const measured = read.info && /\((-?\d+),(-?\d+)\) = \[([\d,]+)\] → \[([\d,]+)\]/.exec(read.info);
if (!measured) {
  bad("② 页面说动了却没给一个实测读数（" + JSON.stringify(read.info) + "）");
} else {
  const mx = Number(measured[1]);
  const my = Number(measured[2]);
  const key = mx + "," + my;
  const freshAfter = await readAt(mx, my);
  const insideBox = mx >= REGION[0] && mx < REGION[0] + REGION[2] && my >= REGION[1] && my < REGION[1] + REGION[3];
  if (!insideBox) {
    bad("② 页面报的取样点不在内容外框里（均匀网格取样就是这种下场 ✗）", { point: [mx, my], box: REGION, page: read.info });
  } else if (spotsBefore1[key] === undefined) {
    bad("② 页面报的点不在判据预先读过的任何一点上", { point: [mx, my], judgeSpots: Object.keys(spotsBefore1) });
  } else if (measured[3] !== spotsBefore1[key]) {
    bad("② 页面念的「前值」与判据应用前的独立读数不等（界面自己算色 ✗）", { page: measured[3], judge: spotsBefore1[key] });
  } else if (measured[4] !== freshAfter) {
    bad("② 页面念的「后值」与判据应用后的独立读数不等", { page: measured[4], judge: freshAfter });
  } else if (measured[3] === measured[4]) {
    bad("② 页面报的前后读数一模一样却宣称「动了」", { value: measured[3] });
  } else {
    ok("② 实测读数逐位对得上判据的独立测量：坐标在内容里 ✓ 前后两值都是服务端报的 ✓", { point: [mx, my], before: measured[3], after: measured[4] });
  }
}
if (after1.effects === before1.effects + 1 && after1.atoms > before1.atoms) {
  ok("应用真的落了对象与原子（服务端数得出来 ✓）", { effects: before1.effects + "→" + after1.effects, atoms: before1.atoms + "→" + after1.atoms });
} else {
  bad("应用后服务端的对象数/原子数不符", { effects: before1.effects + "→" + after1.effects, atoms: before1.atoms + "→" + after1.atoms });
}
const srv1 = (await effects()).find((e) => e.params && e.params.red_hue === 80);
const oid1 = srv1 ? srv1.object_id : null;
const rows1 = (await cardText()).rows;
const shown1 = rows1.find((r) => r.obj === oid1);
if (!oid1) {
  console.error("❌ 判据前提失败：服务端回读里没有 `red_hue:80` 那个对象（① 那次应用没存住）");
  process.exit(3);
}
if (shown1 && /obj=obj_/.test(shown1.text) && /α=/.test(shown1.text)) ok("清单行把对象号与不透明度都念出来（`opacity=0` 那种「挂着但没生效」藏不住 ✓）", { row: shown1.text.slice(0, 70) });
else bad("清单行没把服务端那个对象号念回来（行内「删」也就无处可点）", { server: oid1, rows: rows1.map((r) => r.text.slice(0, 40)) });

// ---------- ④a 用行内「删」清场：删掉真效果 ⇒ 指纹回到基线 ----------
{
  const del = await deleteRow(oid1);
  if (del !== 1) bad("行内「删」按钮没点到", { state: del });
  read = await waitForInfo("已删掉");
  const hashAfterDel = await fingerprint();
  const pixelAfterDel = await readAt(560, 360);
  if (read.info && /⇒\s*整幅合成的服务端指纹变了 ⇒ 画面确实动了/.test(read.info)
    && hashAfterDel === baselineHash && pixelAfterDel === contentPixel) {
    ok("④ 删掉真效果：页面说「动了」✓ 判据核对指纹**回到基线那一个串** ✓ 内容处读数复原 ✓", { hash: hashAfterDel.slice(0, 18), pixel: pixelAfterDel, page: read.info.slice(0, 60) });
  } else {
    bad("④ 删除那一句与服务端事实不符（或指纹没回到基线）", { page: read.info, hashNow: hashAfterDel, hashBase: baselineHash, pixel: pixelAfterDel });
  }
  if ((await effects()).length === 0) ok("删完清单空了（这一层剩 0 个效果 ✓ 页面那句「还剩 0 个」与回读一致 ✓）", { page: read.info.slice(-24) });
  else bad("删完服务端还留着效果对象", { left: (await effects()).length });
}

// ---------- ③ 拼错参数名：收了、存着、画面不动 ⇒ 卡上必须说"空操作" ----------
await setField("effectParams", "{}");
const before3 = { hash: await fingerprint(), atoms: await atoms(), effects: (await effects()).length };
read = await runApply("calibrate_primaries", '{"redhue": 50}', "已新建");
const hashAfter3 = await fingerprint();
const stored3 = (await effects()).find((e) => e.params && e.params.redhue !== undefined);
const serverNoOp3 = hashAfter3 === before3.hash;
if (read.info && /⇒\s*整幅合成逐字节没变 ⇒ 这一笔对画面是空操作/.test(read.info) && serverNoOp3) {
  ok("③ 拼错的参数名：页面说「空操作」✓ 判据核对服务端指纹逐字节没动 ✓（过去这里也念「已应用」✗）", { page: read.info.slice(0, 70) });
  checkNote(read, true, "③′");
  checkNoteWhy(read, "③″");
} else {
  bad("③ 拼错参数名那句没对上服务端事实", { page: read.info, serverNoOp: serverNoOp3 });
}
if (stored3 && read.info && read.info.includes('（参数 {"redhue":50}）')) {
  ok("③ 同时如实念出服务端**收下并存着**的那份参数（「收了」与「动了」分开说 ✓）", { stored: JSON.stringify(stored3.params) });
} else {
  bad("③ 没把服务端原样存着的错参数念回来", { page: read.info, stored: stored3 && stored3.params });
}
if ((await effects()).length === before3.effects + 1 && (await atoms()) > before3.atoms) ok("③ 空操作也照样落了对象与原子（这才需要那个「删」出口 ✓）", { effects: before3.effects + "→" + (await effects()).length });
else bad("③ 空操作没落对象？", { effects: before3.effects + "→" + (await effects()).length });
if (stored3) {
  await deleteRow(stored3.object_id);
  read = await waitForInfo("已删掉");
  if (read.info && /⇒\s*整幅合成逐字节没变 ⇒ 它原本就没参与画面/.test(read.info) && (await fingerprint()) === before3.hash) {
    ok("④ 删掉那个空操作：页面说「它原本就没参与画面」✓ 指纹一个字节都没动 ✓", { page: read.info.slice(0, 70) });
  } else {
    bad("④ 删空操作那句不符", { page: read.info, hashNow: await fingerprint(), hashBase: before3.hash });
  }
}

// ---------- ⑤ 缺省参数就动的 `invert {}`：挡住"一律报空操作"的反向偷懒 ----------
const before5 = await fingerprint();
read = await runApply("invert", "{}", "已新建");
const hashAfter5 = await fingerprint();
if (read.info && /⇒\s*整幅合成的服务端指纹变了 ⇒ 画面确实动了/.test(read.info) && hashAfter5 !== before5) {
  ok("⑤ 缺省参数就动画面的那一个：页面必须说「动了」（不许把结论一律写成空操作 ✓）", { page: read.info.slice(0, 60) });
} else {
  bad("⑤ 默认就动画面的效果被说成没动（或反之）", { page: read.info, serverMoved: hashAfter5 !== before5 });
}
const rowInvert = (await cardText()).rows.find((r) => /invert/.test(r.text));
if (rowInvert) {
  await deleteRow(rowInvert.obj);
  await waitForInfo("已删掉");
  if (await fingerprint() === baselineHash) ok("⑤ 清场：删掉 invert 后指纹回到基线");
  else bad("⑤ 清场后指纹没回到基线（invert 没真删掉？）", { now: await fingerprint(), base: baselineHash });
} else bad("⑤ invert 那行没出现在清单里（清单不列滤镜？）");

// ---------- ⑥ 越界的真参数：就地拒绝、零原子零改动、中文 detail 只在日志 ----------
await setField("effectParams", "{}");
const before6 = { hash: await fingerprint(), atoms: await atoms(), effects: (await effects()).length };
read = await runApply("calibrate_primaries", '{"red_hue": 150}', "未写入");
const after6 = { hash: await fingerprint(), atoms: await atoms(), effects: (await effects()).length };
const refused = read.info && /^未写入：服务端拒绝了这一步（错误码 \S+ · 完整原因见下方日志）。拒绝时不落原子 ⇒ 画面不会动。$/.test(read.info);
if (refused && after6.hash === before6.hash && after6.atoms === before6.atoms && after6.effects === before6.effects) {
  ok("⑥ 越界真参数被服务端拒绝：页面说「未写入」✓ 原子数/效果数/指纹一个都没动 ✓", { page: read.info });
  checkNote(read, false, "⑥′");
} else {
  bad("⑥ 拒绝路径不符（话不对，或其实动了服务端状态）", { page: read.info, hash: before6.hash + "→" + after6.hash, atoms: before6.atoms + "→" + after6.atoms, effects: before6.effects + "→" + after6.effects });
}
// **服务端的逐字中文 `detail` 两头都要问** ✓：日志里必须找得回来（一个字都不许丢 ✗）
// 而卡上不许贴它（贴了就成了英文界面夹整句中文 ✗ 与减淡加深那条同一口径 ✓）。
const logHasDetail = /red_hue 必须在/.test(read.log || "");
if (logHasDetail && !/必须在/.test(read.info || "")) {
  ok("⑥ 服务端的逐字中文 detail 完整留在 `#log` ✓ 卡上不复述（英文界面不夹整句中文 ✓）", { log: (read.log || "").match(/red_hue 必须在[^\n]{0,60}/)[0] });
} else {
  bad("⑥ 拒绝原因两头都不对（日志丢了 detail，或卡上贴了整句中文）", { log: (read.log || "").slice(-140), page: read.info });
}

// ---------- ⑦ 非法 JSON：先拦后送 ⇒ 卡上说没出门，判据核对确实没出门 ----------
await setField("effectParams", "{");
const before7 = { hash: await fingerprint(), atoms: await atoms(), effects: (await effects()).length };
await clearInfo();
await applyNow();
read = await waitForInfo("参数不是合法 JSON");
if (read.info && /^参数不是合法 JSON ⇒ 没有发送请求、没有写入任何改动。$/.test(read.info)) {
  ok("⑦ 非法 JSON：卡上直接说「没有发送请求、没有写入任何改动」（这话只写在折叠日志里等于没说 ✓）", { page: read.info });
  checkNote(read, false, "⑦′");
} else {
  bad("⑦ 非法 JSON 那句没写在卡上（或写成了别的）", { page: read.info });
}
const after7 = { hash: await fingerprint(), atoms: await atoms(), effects: (await effects()).length };
if (after7.hash === before7.hash && after7.atoms === before7.atoms && after7.effects === before7.effects) {
  ok("⑦ 核对：一个原子都没多、效果数与指纹都没动（「没出门」这话兑现了 ✓）", { atoms: before7.atoms, effects: before7.effects });
} else {
  bad("⑦ 非法 JSON 却说没发送，服务端状态却动了", { atoms: before7.atoms + "→" + after7.atoms, effects: before7.effects + "→" + after7.effects });
}

// ---------- ⑧ 点行编辑 ⇒ 「更新这个」：改它而不是再建一个 ----------
await setField("effectParams", "{}");
read = await runApply("calibrate_primaries", '{"red_hue": 80}', "已新建");
const row8 = (await cardText()).rows.find((r) => /calibrate_primaries/.test(r.text));
if (!row8) { console.error("❌ 判据前提失败：编辑场景的行没找到"); process.exit(3); }
const oid8 = row8.obj;
const effectsBefore8 = (await effects()).length;
await evaluate(`(() => {
  const list = document.getElementById("effectsList");
  for (const r of list.children) if ((r.textContent || "").includes("obj=" + ${JSON.stringify(oid8)})) { r.click(); return 1; }
  return 0;
})()`);
await sleep(400);
const editing = await cardText();
if (editing.applyLabel === "更新这个" && editing.params === '{"red_hue":80}') {
  ok("⑧ 点一行进入编辑：按钮文案换「更新这个」✓ 参数框装着**它现在的参数** ✓（不必从头敲 JSON ✓）", { label: editing.applyLabel, params: editing.params });
} else {
  bad("⑧ 点行后没进入编辑态", { label: editing.applyLabel, params: editing.params });
}
await setField("effectParams", '{"red_hue": -80}');
await clearInfo();
await applyNow();
read = await waitForInfo("已更新");
const effectsAfter8 = await effects();
const rowObj8 = effectsAfter8.find((e) => e.object_id === oid8);
if (read.info && /^已更新 调整 calibrate_primaries（参数 \{"red_hue":-80\}）⇒/.test(read.info)
  && effectsAfter8.length === effectsBefore8 && rowObj8 && rowObj8.params.red_hue === -80) {
  ok("⑧ 更新走的是 `update_adjustment`：对象数一个没多 ✓ 回读参数是新值 ✓ 页面说「已更新」✓", { page: read.info.slice(0, 60), effects: effectsAfter8.length });
} else {
  bad("⑧ 更新路径不符（新建了一个？或参数没改过来？）", { page: read.info, effects: effectsBefore8 + "→" + effectsAfter8.length, stored: rowObj8 && rowObj8.params });
}
await deleteRow(oid8);
await waitForInfo("已删掉");

// ---------- ⑨ 英文模式：回执行整句无汉字、数字仍是服务端那一个 ----------
await gotoLang("en");
const enShell = await openCard();
if (enShell.infoVisible) ok("英文模式下卡片同样能展开、结论区可见");
else bad("英文模式下结论区不可见", enShell);
await setField("effectParams", "{}");
read = await runApply("calibrate_primaries", '{"red_hue": 80}', /^(Added|Nothing)/);
const HAN = /[一-鿿]/;
const enLine = /^Added adjustment calibrate_primaries \(params \{"red_hue":80\}\) ⇒ the server's whole-composite fingerprint changed ⇒ the picture really moved · measured \(\d+,\d+\) = \[[\d,]+\] → \[[\d,]+\] · this layer now holds \d+ effect\(s\)$/;
if (read.info && enLine.test(read.info) && !HAN.test(read.info)) {
  ok("⑨ 英文模式回执行整句翻译 ✓ 坐标与 RGBA 还是服务端那几个数 ✓", { page: read.info });
} else {
  bad("⑨ 英文模式回执行没整句翻译（或数字走样、或夹汉字）", { page: read.info });
}
const enRows = await cardText();
const enRow = enRows.rows.find((r) => /calibrate_primaries/.test(r.text));
if (enRow && enRow.del && !HAN.test(enRow.del.text) && !HAN.test(enRow.title || "") && !HAN.test(enRow.del.title || "")) {
  ok("⑨ 行内「删」按钮、行 tooltip、删除按钮 title 在英文模式下都是英文", { del: enRow.del.text, title: enRow.title });
} else {
  bad("⑨ 英文模式的清单行还留着汉字（按钮或 tooltip 漏词条）", { row: enRow && { text: enRow.text, title: enRow.title, del: enRow.del } });
}
if (enRow) {
  await deleteRow(enRow.obj);
  read = await waitForInfo(/Deleted/);
  if (read.info && /^Deleted calibrate_primaries \(object obj_\w+\) ⇒ the server's whole-composite fingerprint changed ⇒ the picture really moved · this layer now has 0 effect\(s\) left$/.test(read.info) && !HAN.test(read.info)) {
    ok("⑨ 英文模式的删除句也是整句英文（「画面确实动了」这一支翻到位 ✓）", { page: read.info });
  } else {
    bad("⑨ 英文模式删除句没翻到位", { page: read.info });
  }
}

// ---------- ⑩ 关掉服务器渲染：拿不到指纹就不许宣称结论 ----------
// **走真实用户那条路** ✓：URL **不带** `render` 参数 ⇒ 「服务器渲染」勾选框说了算 ✓
// 带着 `render=server` 去点勾选是没用的 ✗（`serverRenderPreferred()` 里 **URL 那一层盖过
// `localStorage`** ✓ 首轮实测点完 reload 又被拉回服务端 ⇒ "本地那一臂"根本没跑到 ✓
// 那是判据自己走错门 ✗ 不是产品红 ✓）。
// **还有一个坑** ✓：勾选框的 `checked` 与 `change` 监听是 `setupFileMenu()` 里才挂的 ✗
// 静态 HTML 里它是**未勾**的 ⇒ boot 跑完之前点它**什么也不会发生** ✓
// 于是这里先把偏好钉成"开"（`"1"` ✓）⇒ **勾选框亮起来**就是"监听已挂上"的确定信号 ✓
// （"开机完成前那个勾选框跟实际路由不一致"这件事本身也记进报告 ✓）。
await evaluate(`(() => { try { localStorage.setItem("yanshi.serverRender", "1"); } catch (error) {} return 1; })()`);
await gotoLang("zh", { render: "none", needLayer: false });
let wired = false;
for (let i = 0; i < 80; i += 1) {
  wired = await evaluate(`(() => { const b = document.getElementById("useServerRender"); return !!b && b.checked === true; })()`);
  if (wired) break;
  await sleep(250);
}
if (!wired) {
  skip("没等到「服务器渲染」勾选框跟着偏好亮起来（boot 没走完）⇒ 这一臂没测到 ✗ 不拿它冒充结论");
} else {
  await evaluate(`(() => { const b = document.getElementById("useServerRender"); if (b.checked) b.click(); return 1; })()`);
  try { await waitEvent("Page.loadEventFired", 20000); } catch (error) { /* 有的环境不再发这个事件 ✓ 后面有轮询兜着 ✓ */ }
  let off = null;
  for (let i = 0; i < 80; i += 1) {
    off = await evaluate(`(() => {
      if (/[?&]render=/.test(location.search)) return "URL-OVERRIDES";
      const b = document.getElementById("useServerRender");
      if (!b) return null;
      return b.checked === false && localStorage.getItem("yanshi.serverRender") === "0" ? true : false;
    })()`);
    if (off === true) break;
    await sleep(250);
  }
  if (off !== true) {
    skip("没把渲染切到本地（勾选框状态实为 " + JSON.stringify(off) + "）⇒ 这一臂没测到 ✗ 不拿它冒充结论", { state: off });
  } else {
    read = await runApply("calibrate_primaries", '{"red_hue": 80}', /已新建|未写入|本地内核|没法验证/);
    const claim = read.info && /画面确实动了|这一笔对画面是空操作/.test(read.info);
    if (claim) bad("⑩ 关掉服务端渲染后，页面在没有服务端指纹的情况下宣称了结论", { page: read.info });
    // **⑩b 责任要说给走过的那条路**（本轮实测抓出来的真 bug ✗）：关掉「服务器渲染」后
    // 请求走的是**本地内核** ✓ 而回执句原先把**所有**失败都写成"**服务端**拒绝了这一步" ✗
    // ⇒ 读的人去翻服务端日志 ✓ 该看的却在本地内核 ✓ 而"页面 == 回执"那种比对**看不见它**
    //（两边都写着服务端 ✓）。所以这一臂必须**点名禁止**「服务端拒绝」这四个字 ✗
    // 同时 ⑥（服务端那一臂）继续钉住「服务端拒绝」⇒ 两臂合起来才把"话跟着路走"钉死 ✓。
    else if (read.info && /服务端拒绝/.test(read.info)) bad("⑩b 这一笔走的是本地内核，页面却把失败说成「服务端拒绝」⇒ 把人送去翻服务端的日志", { page: read.info });
    else if (read.info && /没法验证画面动没动|本地内核没有这个端点|本地内核拒绝了这一步/.test(read.info)) ok("⑩ 本地那一臂：页面只说「本地内核…」或「没法验证」✓ 不装作验证过、也不冤枉服务端", { page: read.info.slice(0, 70) });
    else bad("⑩ 本地那一臂的回执句不在诚实话里", { page: read.info });
    // **⑩c 错误码要念响应里真有的那一个**（同一轮实测抓出来的 ✓）：服务端把码放在
    // `error_code` ✓ 本地内核那一臂放在 `error` ✓（`api-local.js` 的 `p2Bad` 形状 ✓）。
    // 卡片原先只读 `error_code` ⇒ 本地那一臂念成 `unknown` ✗ 而**同一页面**的通用失败分支
    // 两个都读 ⇒ 日志里明明白白写着具体的码 ⇒ **同一事件两个码** ✗。
    // 判据不写死码名 ✓ 从日志里那个**通用分支**写的句子取码 ✓ 再要求卡上那句与它同一个串 ✓
    //（比对的是两条**独立**的代码路径 ⇒ 卡片退回只读 `error_code` 就红 ✓）。
    const cardCode = (/错误码 (\S+)/.exec(read.info || "") || [])[1];
    const genericCodes = [...String(read.log || "").matchAll(/错误 (\S+?)：/g)].map((mm) => mm[1]);
    if (cardCode && cardCode !== "unknown" && genericCodes.includes(cardCode)) {
      ok("⑩c 本地那一臂的错误码与日志同源（响应里那个具体码 ✓ 不是 unknown 占位符 ✓）", { code: cardCode, seen: [...new Set(genericCodes)].slice(-4) });
    } else if (cardCode) {
      bad("⑩c 卡上的错误码与日志不同源（界面漏读了响应里的码）", { card: cardCode, log: [...new Set(genericCodes)].slice(-4) });
    }
    // **⑩d 清单那一格也要说清是"谁"没读到**（第七轮 ★ 第二次看真截图抓到的 ✗）：`10` 那张上 ✓
    // 回执行已经说实话（「未写入：**本地内核**拒绝了这一步」✓ ⑩b 的成果）✓ 而**紧接底下那一行**是
    // 「读取失败：endpoint_not_local」⇒ 一个**裸码** ✗ 同一张卡上同一件事 ✓ 一句说责任一句不说 ✓
    // 人只能猜是"服务端读不到我的效果"还是"本机压根没这端点"（后者才是事实 ✓ 见登记 ⑬/㉟）⇒
    // 现在那一句走 ⑩b 那同一对 `refusalVenue()`／`failureCode()` ✓ 这条断言钉它。
    // **三种屏幕状态各有说法**（不留"什么都没发生也算通过"的空档 ✓ 与第六轮 ㉛ 同族规矩）：
    //   ① 那句是「读取失败…」⇒ 本条**有东西可问** ✓ 必须同时带 venue 与具体码 ✓ 否则产品红；
    //   ② 那格念出了效果清单 ⇒ **本地内核答得出 `list_effects`** ⇒ 与「本地没有这个端点」矛盾 ✓
    //      这不是产品红而是**判据前提变了** ⇒ 按本文件口径走 `exit(3)`（AGENTS.md：不许把环境／
    //      前提问题判成产品红 ✓ 也不许把它判成产品绿 ✓）；
    //   ③ 那格空着 ⇒ 同样问不到东西 ⇒ `exit(3)` ✓ 不静默绿。
    const listLine = String(read.listText || "");
    if (/^读取失败/.test(listLine)) {
      const codeInList = (/：(\S+)$/.exec(listLine) || [])[1] || "";
      if (listLine.includes("本地内核") && codeInList && codeInList !== "unknown") {
        ok("⑩d 本地那一臂读不到清单时，那一句也说清了是谁没读到、码是哪个（与回执同一族口径 ✓）", { line: listLine.slice(0, 64) });
      } else {
        bad("⑩d 清单读取失败那一句缺责任归属或缺码 ⇒ 屏幕上只剩一个裸码（把人送去翻服务端的日志）", { line: listLine.slice(0, 64) });
      }
    } else if (listLine) {
      console.error("❌ 判据前提失败：本地那一臂的清单格念出了效果清单 ⇒ 本地内核答得出 `list_effects`？与登记 ⑬／㉟ 矛盾 ⇒ 本条要重写 ✗ 不算产品红也不算绿（实为 " + listLine.slice(0, 64) + "）");
      process.exit(3);
    } else {
      console.error("❌ 判据前提失败：本地那一臂的清单格空着 ⇒ ⑩d 问不到东西（既没说读失败也没念清单）✗ 不静默绿");
      process.exit(3);
    }
    await gotoLang("zh");
  }
}

// ---------- 全程 0 条 JS 错误 ----------
if (pageErrors.length === 0) ok("全程 0 条 JS 错误");
else bad("页面抛了 JS 错误", pageErrors.slice(0, 3));

// ---------- 收尾：服务端状态回到基线（不留一堆测试对象）----------
const tail = await fingerprint();
if (tail === baselineHash) ok("跑完一轮：整幅合成指纹回到基线（页面这些操作真没在画面上留东西 ✓）");
else bad("跑完后画面没回到基线（删除出口漏了谁？）", { now: tail, base: baselineHash });

await send("Target.closeTarget", { targetId: created.result.targetId });
ws.close();
if (problems.length) {
  console.log("判据失败 " + problems.length + " 条：\n  · " + problems.join("\n  · "));
  process.exit(1);
}
console.log("✓ 调整/滤镜卡说实话：动没动由服务端整幅合成指纹判、实测读数与判据同源且落在内容上、错参数说成空操作、拒绝与非法 JSON 一根汗毛没动、删有出口且能回到基线、更新不新建、读不到也说清是谁没读到、英文整句无汉字");
process.exit(0);
