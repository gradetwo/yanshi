#!/usr/bin/env node
// **判据：减淡加深（`dodge_burn_setup`）在查看器里必须说真话、并且真的建得出** ✓。
//
// **抓的是哪一类缺口** ✓：这工具早前在页面上**零入口**（`grep -c dodge_burn crates/yanshi-http/`
// 命中 **0** ✗）⇒ 局部提亮/压暗只能靠 Agent 调工具 ✗。补上卡片之后，新的风险换成
// **"界面说的话与服务端的事实不同源"** ✓ 而这一族缺陷本轮是真刀真枪抓出来的（见下）✓：
//   ① 灰层当年铺的是 50% 显示灰 **128** ✗ 而本引擎 `soft_light` 在**线性缓冲**上做 ⇒
//      中性字节是 **188** ⇒ "点一下建组"这一步自己就把 128 的底色压到 **97**（整幅暗 31 级 ✓）；
//   ② `mode=curves` 建组后**画面一动不动**（当时调整对象只作用于自己图层的缓冲 ✓）
//      却回 `ok: true` ⇒ 静默 ✗ 当时服务端回 `effective:false` ✓ 界面把这句摆在卡片上 ✓；
//      **第六轮翻转** ✓：上游 `302d414d`「纯调整层渲染时先用下方内容播种」之后 curves
//      **真的改得到画面** ⇒ 服务端回 `effective:true` ✓ 界面那句 ⚠ 于是要**跟着撤** ✗
//      留着它就成了新的谎 ✓ 本条 ⑦ 现在钉的就是"页面必须与同一时刻的回执同源"✓；
//   ③ 组名、层数、中性字节这些数**只能来自服务端回执** ✓ 界面自己拼一个"看起来对"的字符串 ✗
//      就变成第二个真值（下一轮服务端改名 ⇒ 界面还在念旧的 ✓）；
//   ④ **本条判据第一次跑红抓到的东西**（不是先验写进去的 ✗）：`tools_retouch.rs` 的**复用分支**
//      整格漏掉 `mode` ⇒ 英文模式下界面念出 `Reusing existing group … · method undefined` ✓
//      ⇒ 而"页面 == 回执"那种比对**看不见它**（回执本来就缺那一格 ✓ 两边一起缺 ✓）
//      ⇒ 修在服务端（复用回执照**磁盘**报实际那条路 ✓ 顺带修掉"灰组被请求成 curves 时
//      回执说它改不到画面"那个方向反了的谎 ✓）＋界面兜一句`未知` ✓
//      ⇒ 判据里那条 `③b` 与 `方法 gray` 的正则就是它的守卫 ✓。
// ⇒ 所以本条**不断言像素本身**（那是 `tool-dodge-burn-effective.mjs` 的活 ✓），
//   它断言的是**页面上写出来的每一个数都能在同一时刻从服务端读回来** ✓ 以及**该拦的路径真没出门** ✓。
//
// **钉住的九件事**：
//   ① 卡在位 ✓ 「源图层」下拉的 id 序列与 `list_layers` **逐一等**（顺序也等 ✓）；
//   ② 「这次会发生什么」那句**预测**（将新建组 `「前缀 · 减淡加深」`、放在源上方）
//      按下去之后**服务端事实照着它兑现**：组名逐字相同 ✓ 且组的 `z_index` 是**紧贴源之上**的那一档 ✓；
//   ③ 回执行 `#dodgeBurnInfo` 的**整串**由服务端回执拼出来 ⇒ 判据把同一组参数**再直调一次**
//      （复用路径 ⇒ 字段与首次一致 ✓）⇒ 逐位对上：组 id、层数与 id、对象数与 id、蒙版角色、中性字节 ✓；
//   ④ 灰层法建组后 `sample_colors` 的底色**仍是同一个数**（≤1 LSB ✓ 界由 `tool-*` 那条实测给 ✓）
//      ⇒ "点一下不会把画弄黑"这件事在**页面走的这条路径**上也成立 ✓；
//   ⑤ 建完的新组与灰层必须**回到下拉里**（结构改动后界面自己刷新 ✓ 不许让人刷新页面 ✓）；
//   ⑥ 中点越界（`curves` + 1.5）⇒ **就地拒绝**：`#dodgeBurnInfo` 念出区间 ✓ **零新原子** ✓ **零新层** ✓；
//   ⑦ `curves` 合法中点 ⇒ 卡片**必须与回执同一口径**：服务端回 `effective=true` ✓ 页面上就
//      **不许**出现「不会改变画面」⚠（也不许念 curves 根本没有的「中性字节」✓）
//      而服务端的逐字 `note` 必须**同时**躺在 `#log` 里 ✓（界面不复述 ✗ 也不丢 ✓）；
//   ⑧ 同名二次点击 ⇒ 页面念"复用既有组" ✓ 服务端层数与原子数**一个都没多** ✓；
//   ⑨ 切英文 ⇒ ②/③/⑥ 那三行动态句子**整句翻译**（组名的固定尾巴翻 ✓ 用户自己填的前缀不翻 ✓）
//      且**不出现汉字**；`created`/`opened` 这两格 HTTP 层字段**不许被念成"建了几层"** ✓；全程 0 条 JS 错误 ✓。
//
// **口径**：每个场景用各自的组名前缀 ⇒ 断言互不干扰 ✓；导航 URL 上**钉死** `render=server` ✓
//（`run-criteria.sh` 对 browser-* 统一注入 `&render=client` ✗ 而本条走的是服务端工具 ⇒
// 已把它加进那条**例外名单** ✓；判据自己也再显式带一次 ✓ 双保险 ✓）；
// 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)` ✓（AGENTS.md：不许把环境问题判成产品红 ✓）。
//
// 用法：node scripts/browser-dodge-burn.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-dodge-burn.mjs http://127.0.0.1:15900?doc=x&token=y 9490
import { setTimeout as sleep } from "node:timers/promises";

// **★ 总超时守卫 ✗ ★**（本仓约定 ✓）：要连 CDP 浏览器 ⇒ 起不来时会**一直等** ✗ 拖住 CI 分片 ✓。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 180000);

const url = process.argv[2];
// **★ 端口优先读环境变量 ✗ ★**（`browser-reference-overlay.mjs` 那处教训 ✓）：
//   `run-criteria.sh` 给 browser-* 传 `<viewer-url> <cdpPort>` ⇒ argv[3] 是端口 ✓；
//   但手工跑时常把 doc URL 整个丢在 argv[3] ✗ ⇒ 只认纯数字 ✓ 否则退回 9333 ✓。
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
const base = url ? new URL(url).origin : "http://127.0.0.1:15900";
if (!url) {
  console.error("用法: node scripts/browser-dodge-burn.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

// ---------- 一次性文档 + 夹具 ----------
const docId = "t_db_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 200, height: 150 }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());
const SRC = "SRC";
const made = await callTool("create_layer", { layer_id: SRC, name: "底" });
if (!made.ok) { console.error("❌ 判据前提失败：建源图层 ⇒ " + JSON.stringify(made).slice(0, 200)); process.exit(3); }
// **平涂 128** ✓：底色就在中性点上 ⇒ 铺错灰（当年那个 128 ✗）会立刻把这一格压到 97 ✓ 露出来 ✓。
const filled = await callTool("fill", { layer_id: SRC, data: { color: "#808080", region: { x: 0, y: 0, w: 200, h: 150 } } });
if (!filled.ok) { console.error("❌ 判据前提失败：铺底色 ⇒ " + JSON.stringify(filled).slice(0, 200)); process.exit(3); }
const atomCount = async () => Number((await callTool("get_log", {})).count || 0);
const layerRows = async () => (await callTool("list_layers", {})).layers;
const seedAtoms = await atomCount();
const seedRows = await layerRows();
if (!(seedAtoms > 0 && seedRows.length === 2)) {
  console.error("❌ 判据前提失败：夹具没就位（原子 " + seedAtoms + "｜层 " + seedRows.length + "）");
  process.exit(3);
}
const basePixel = (await callTool("sample_colors", { points: [[20, 20]] })).samples[0]?.rgba;
if (!Array.isArray(basePixel) || basePixel[0] !== 128) {
  console.error("❌ 判据前提失败：底读回来不是 128 ⇒ " + JSON.stringify(basePixel));
  process.exit(3);
}

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
/** **擦偏好再进文档** ✓：`localStorage` 按源共享 ✗ 残留 `yanshi.serverRender="0"` 会让判据
 *  拿着**本地路由的失败**当产品结论 ✓（`browser-frequency-separation.mjs` 实测踩过 ✓）。 */
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(500);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); return 1; })()`);
const gotoLang = async (lang) => {
  await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=${lang}&render=server&v=${Date.now()}` });
  await waitEvent("Page.loadEventFired");
  let booted = false;
  for (let i = 0; i < 60; i += 1) {
    booted = await evaluate(`!!(window.yanshi && document.getElementById("dodgeBurnSource")?.options.length >= 2)`);
    if (booted) break;
    await sleep(250);
  }
  if (!booted) { console.error("❌ 判据前提失败：开机后「源图层」下拉没装着至少 2 个图层（lang=" + lang + "）"); process.exit(3); }
  // **有界等待这条前提** ✓（登记 ⑰ 同族 ✓）：单次读可能读到**上一次导航留下的页面** ✗。
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
  const langNow = await evaluate(`document.documentElement.lang`);
  if (langNow !== (lang === "en" ? "en" : "zh-CN")) {
    console.error("❌ 判据前提失败：语言没落在 " + lang + "（实为 " + JSON.stringify(langNow) + "）");
    process.exit(3);
  }
};
const setField = (id, value) => evaluate(`(() => {
  const el = document.getElementById(${JSON.stringify(id)});
  if (!el) return "no-el";
  el.value = ${JSON.stringify(String(value))};
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  return el.value;
})()`);
const pickSource = (layerId) => setField("dodgeBurnSource", layerId);
const pickMode = (mode) => setField("dodgeBurnMode", mode);
const setName = (name) => setField("dodgeBurnName", name);
const setMid = (id, v) => setField(id, v);
const run = () => evaluate(`(() => { const b = document.getElementById("dodgeBurnRun"); if (!b) return "no-button"; b.click(); return 1; })()`);
/** 卡片上那两行动态句（`#dodgeBurnCurrent` = 预测 ✓ `#dodgeBurnInfo` = 回执 ✓）。 */
const cardText = () => evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  const t = (id) => (q(id) ? q(id).textContent : null);
  return JSON.stringify({
    current: t("dodgeBurnCurrent"),
    info: t("dodgeBurnInfo"),
    options: q("dodgeBurnSource") ? [...q("dodgeBurnSource").options].map((o) => o.value) : [],
    log: t("log"),
  });
})()`).then((s) => JSON.parse(s));
/** **等回执行变成"预期的形状"** ✓（有界 ✓）：点击是异步的 ✓
 *  而 `sleep(固定)` 要么太快（读到上一句 ✗ 判据假红 ✓）要么白等 ✗。 */
const waitForInfo = async (pattern, tries = 40) => {
  for (let i = 0; i < tries; i += 1) {
    const read = await cardText();
    if (read.info && (typeof pattern === "string" ? read.info.includes(pattern) : pattern.test(read.info))) return read;
    await sleep(250);
  }
  return cardText();
};

await gotoLang("zh");

// ---------- ① 卡片在位 ✓ 下拉与服务端图层清单逐一对应 ----------
const shell = await cardText();
if (!(await evaluate(`!!document.getElementById("cardDodgeBurn") && !!document.getElementById("dodgeBurnRun")`))) {
  bad("减淡加深卡或「建组」按钮不在页面上");
} else {
  ok("减淡加深卡在页面上，「建组」按钮在位");
}
const serverIds = (await layerRows()).map((l) => l.layer_id);
if (JSON.stringify(shell.options) === JSON.stringify(serverIds)) ok("「源图层」下拉的 id 序列与 `list_layers` 逐一等", { count: serverIds.length });
else bad("下拉与服务端图层清单不同源", { page: shell.options, server: serverIds });
await pickSource(SRC);

// ---------- ② 预测 → 兑现：组名逐字相同、且紧贴源之上 ----------
const GROUP = "「p2 · 减淡加深」";
await setName("p2");
await pickMode("gray");
let read = await cardText();
if (read.current === "将新建组 " + GROUP + "（放在「底」上方）") ok("卡片先念出「这次会新建哪个组」，字符串与服务端命名规则一致", { page: read.current });
else bad("建组前的预测句不对（界面在自拼字符串？）", { page: read.current });
const atomsBefore2 = await atomCount();
const layersBefore2 = (await layerRows()).length;
await run();
read = await waitForInfo("已建组 ");
const after2 = await layerRows();
const group2 = after2.find((l) => l.name === GROUP);
const srcRow = after2.find((l) => l.layer_id === SRC);
const aboveZ = after2.filter((l) => l.z_index > srcRow.z_index).map((l) => l.z_index).sort((a, b) => a - b);
if (group2 && group2.kind === "layer_group" && aboveZ[0] === group2.z_index) {
  ok("点「建组」后服务端真的建出了那个**逐字同名**的组，并且它紧贴源图层之上", { group: group2.name, z: group2.z_index, srcZ: srcRow.z_index });
} else {
  bad("建组的名称或层位与服务端事实不符", { page: read.current, group: group2 && [group2.name, group2.kind, group2.z_index], srcZ: srcRow && srcRow.z_index, aboveZ });
}
if (group2 && after2.filter((l) => l.parent_id === group2.layer_id && l.blend_mode === "soft_light").length === 1) {
  ok("组里那一张灰层是 `soft_light`（界面念的「灰层法」在服务端兑现了）");
} else {
  bad("组里没有 soft_light 灰层", { kids: after2.filter((l) => l.parent_id === group2?.layer_id).map((k) => [k.name, k.blend_mode]) });
}
if ((await atomCount()) > atomsBefore2 && (await layerRows()).length === layersBefore2 + 2) {
  ok("建组确实落了原子、确实多出「组 + 灰层」两层", { atoms: atomsBefore2 + "→" + (await atomCount()), layers: layersBefore2 + "→" + (await layerRows()).length });
} else {
  bad("建组后服务端的原子/层数不符", { atoms: atomsBefore2 + "→" + (await atomCount()), layers: layersBefore2 + "→" + (await layerRows()).length });
}

// ---------- ③ 回执行的每一个数都与服务端同源 ----------
const direct = await callTool("dodge_burn_setup", { layer_id: SRC, name: "p2", mode: "gray" });
if (!direct.ok || direct.already_exists !== true) {
  console.error("❌ 判据前提失败：直调复用路径没回 already_exists ⇒ " + JSON.stringify(direct).slice(0, 200)); process.exit(3);
}
const ids = (key) => ((direct[key] && direct[key].join) ? direct[key].join(",") : "无");
// **`方法` 那一格取回执的 `mode`** ✓（本轮实测：漏它 ⇒ 界面念 `方法 undefined` ✗
//  判据如果**自己把 "gray" 写死**在期望串里 ✗ 就只会红在"页面少一个词"上 ✗ 看不出是**回执漏格** ✓
//  ⇒ 这里让期望串跟着回执走 ✗ 回执漏 ⇒ 期望串也缺 ⇒ 于是**比对齐了但两边都是 undefined** ✗
//  所以再补一条**独立**的断言：`direct.mode === "gray"` ✓ 漏格当场红 ✓ 见 ③b ✓）
const expectLine = "已建组 " + direct.group_id + " · 方法 " + direct.mode + " · 层 " + (direct.layer_ids || []).length + " 个（" + ids("layer_ids") + "）"
  + " · 对象 " + (direct.object_ids || []).length + " 个（" + ids("object_ids") + "）"
  + " · 蒙版 减淡 " + (direct.masks?.dodge || "无") + " · 加深 " + (direct.masks?.burn || "无")
  + " · 灰层中性字节 " + direct.neutral_gray;
read = await cardText();
if (read.info === expectLine) ok("回执整句由服务端字段拼成：组 id、层/对象数与 id、蒙版角色、中性字节逐位对上", { page: read.info });
else bad("卡片回执与服务端回执不同源", { page: read.info, expect: expectLine });
if (direct.mode !== "gray") bad("③b 服务端复用回执这次没把 mode 念回来（界面那句「方法」会落成 undefined）", { server: direct.mode });
else ok("③b 复用回执带 mode=gray（界面那句「方法」有这个数可念）", { mode: direct.mode });
if (direct.neutral_gray !== 188) bad("服务端这次给的不再是 188（本引擎 `soft_light` 的中性）——判据里 ④ 的口径要跟着重测", { server: direct.neutral_gray });
else ok("中性字节来自回执（界面没写死）", { neutral: direct.neutral_gray });
if (/created|opened/.test(read.info || "")) bad("回执行念了 HTTP 层注入的 created/opened（那两格不是「建了几层」）", { page: read.info });
else ok("回执行没把 created/opened 当效果念");

// ---------- ④ 页面这条路径建完组后，底色仍是那个底色 ----------
const afterPixel = (await callTool("sample_colors", { points: [[20, 20]] })).samples[0]?.rgba;
const drift = Array.isArray(afterPixel) ? Math.abs(afterPixel[0] - basePixel[0]) : 999;
if (drift <= 1) ok("灰层法建组后合成读数没动（" + basePixel[0] + "→" + afterPixel[0] + " ✓ ≤1 LSB）", { drift });
else bad("灰层建组自己就改变了画面（中性铺错了？" + basePixel[0] + "→" + afterPixel[0] + "）", { drift });

// ---------- ⑤ 结构改动后，新组要回到下拉里（不用人刷新页面） ----------
read = await cardText();
if (group2 && read.options.includes(group2.layer_id)) ok("建完组后界面自己刷新：新组出现在「源图层」下拉里", { options: read.options.length });
else bad("建完组后下拉还装着旧清单（界面没刷新图层结构）", { page: read.options, group: group2 && group2.layer_id });

// ---------- ⑥ 中点越界：就地拒绝，零原子零新层 ----------
const atomsBefore6 = await atomCount();
const layersBefore6 = (await layerRows()).length;
await pickMode("curves");
await setMid("dodgeBurnDodge", "1.5");
await setName("p6");
await run();
await sleep(900);
read = await cardText();
if (/0\.05–0\.95/.test(read.info || "") && (await atomCount()) === atomsBefore6 && (await layerRows()).length === layersBefore6) {
  ok("中点 1.5 被就地拒绝：说出区间、没发请求、一个原子都没写", { page: read.info, atoms: atomsBefore6, layers: layersBefore6 });
} else {
  bad("中点越界没被就地拒绝", { page: read.info, atoms: atomsBefore6 + "→" + (await atomCount()), layers: layersBefore6 + "→" + (await layerRows()).length });
}

// ---------- ⑦ curves：这条路**改得到画面** ⇒ 卡片照回执念，界面不许自己留旧的 ⚠ ----------
// 第五轮在这里钉的是"effective=false 必须摆在卡片上"✓ 而**那句话本身随着上游 `302d414d`
// （纯调整层渲染时先用下方内容播种）变成了假的** ⇒ 断言**翻向** ✓ 翻完仍在问同一件事：
// **页面上那一句必须与服务端同一时刻的回执同源** ✓
// 于是三件都要问：① 服务端说 `effective=true` ✓ 页面就**不许**出现「不会改变画面」⚠
// （谁把旧的 ⚠ 按 `mode` 硬编码在界面上 ✗ 这条红 ✓）；② curves 组没有灰层 ⇒ 页面不许念
// 「中性字节」✓（念了就是界面自己拼的第二真值 ✗）；③ 服务端那句逐字 `note` 必须**完整**
// 躺在 `#log` 里 ✓ 界面不复述 ✗ 但一个字都不许丢 ✓。
await setMid("dodgeBurnDodge", "0.62");
await setMid("dodgeBurnBurn", "0.38");
await setName("p7");
await run();
read = await waitForInfo("方法 curves");
const curvesDirect = await callTool("dodge_burn_setup", { layer_id: SRC, name: "p7", mode: "curves", dodge_mid: 0.62, burn_mid: 0.38 });
// 服务端**自己**的 fresh 回执（换个组名另建一套 ⇒ `note` 与名字无关 ✓ 于是可比 ✗ 判据不抄文案）。
const curvesFresh = await callTool("dodge_burn_setup", { layer_id: SRC, name: "p7f", mode: "curves", dodge_mid: 0.62, burn_mid: 0.38 });
read = await cardText();
if (curvesDirect.ok !== true) {
  console.error("❌ 判据前提失败：直调 curves 建组 ⇒ " + JSON.stringify(curvesDirect).slice(0, 200));
  process.exit(3);
}
if (!(curvesDirect.effective === true && curvesDirect.ineffective_reason === null)) {
  bad("服务端 curves 这条路应当 effective=true 且 ineffective_reason=null（上游 302d414d 起纯调整层播种下方内容）", { server: { effective: curvesDirect.effective, reason: curvesDirect.ineffective_reason } });
} else {
  ok("服务端回执：curves effective=true · ineffective_reason=null");
}
if (read.info && /方法 curves/.test(read.info)
  && !/不会改变画面|effective=false/.test(read.info)
  && !/中性字节/.test(read.info) && curvesDirect.effective === true) {
  ok("曲线法建组后卡片念「方法 curves」且**没有**旧的「不会改变画面」⚠（界面只转述回执 ✗ 不按 mode 自己加警告 ✓）", { page: read.info.slice(0, 90) });
} else {
  bad("曲线法那行的卡片话与回执不同源（该去掉的 ⚠/中性字节还在）", { page: read.info, serverEffective: curvesDirect.effective });
}
if (curvesFresh.ok === true && curvesFresh.note && (read.log || "").includes(curvesFresh.note)) {
  ok("服务端的逐字说明完整留在 `#log`（界面不复述 ✗ 但一个字没丢）", { note: curvesFresh.note.slice(0, 60) });
} else {
  bad("日志里没有服务端那句 note", { note: (curvesFresh.note || "").slice(0, 60), log: (read.log || "").slice(-160) });
}

// ---------- ⑧ 同名二次点击：页面念"复用"，服务端一个都没多 ----------
const atomsBefore8 = await atomCount();
const layersBefore8 = (await layerRows()).length;
await pickMode("gray");
await setName("p2");
read = await cardText();
if (read.current === "组名 " + GROUP + " 已存在 ⇒ 这次会复用既有组、不新建（要重做请先删掉它）") ok("换成同名组前缀时，预测句跟着改成「会复用」", { page: read.current });
else bad("同名组的预测句没改口", { page: read.current });
await run();
read = await waitForInfo("复用既有组 ");
// **`方法 gray` 必须在复用那一句里** ✓（本轮实测红过一次：复用回执漏 `mode` ⇒ 界面念
// "方法 undefined" ✓ 上面几条断言**全都看不见** ✓ 因为它们比的是"页面 == 回执"✗
// 而回执本身就缺那一格 ✓ ⇒ 必须**单独点名要求这一格在场** ✓）。
if (/^复用既有组 \S+ · 方法 gray · /.test(read.info || "") && (await atomCount()) === atomsBefore8 && (await layerRows()).length === layersBefore8) {
  ok("二次点击真的复用：页面念「复用既有组」，服务端层数与原子数一个没多", { page: (read.info || "").slice(0, 40) });
} else {
  bad("复用路径不符", { page: read.info, atoms: atomsBefore8 + "→" + (await atomCount()), layers: layersBefore8 + "→" + (await layerRows()).length });
}

// ---------- ⑨ 英文模式：三行动态句整句翻，且不留汉字 ----------
await gotoLang("en");
await pickSource(SRC);
await pickMode("gray");
await setName("p2");
read = await cardText();
const HAN = /[\u4e00-\u9fff]/;
if (read.current && /^Group name "p2 · Dodge and burn" already exists/.test(read.current) && !HAN.test(read.current)) {
  ok("英文模式下预测句整句翻译（组名尾巴翻 ✓ 用户填的前缀没被翻 ✓）", { page: read.current });
} else {
  bad("英文模式的预测句没翻到位", { page: read.current });
}
await run();
read = await waitForInfo(/Reusing existing group|Grey layer neutral byte|neutral byte/);
if (read.info && /Reusing existing group .* · method gray .* · grey layer neutral byte 188$/.test(read.info) && !HAN.test(read.info)) {
  ok("英文模式的回执整句翻译，数字仍是服务端那一个", { page: read.info });
} else {
  bad("英文模式的回执句没翻到位（或漏了汉字）", { page: read.info });
}
await pickMode("curves");
await setMid("dodgeBurnDodge", "1.5");
await run();
await sleep(900);
read = await cardText();
if (read.info && /0\.05.0\.95/.test(read.info) && !HAN.test(read.info)) ok("英文模式下越界拒绝句也是整句英文", { page: read.info });
else bad("英文模式的越界拒绝句没翻", { page: read.info });

// ---------- 全程 0 条 JS 错误 ----------
if (pageErrors.length === 0) ok("全程 0 条 JS 错误");
else bad("页面抛了 JS 错误", pageErrors.slice(0, 3));

await send("Target.closeTarget", { targetId: created.result.targetId });
ws.close();
if (problems.length) {
  console.log("判据失败 " + problems.length + " 条：\n  · " + problems.join("\n  · "));
  process.exit(1);
}
console.log("✓ 减淡加深卡片说实话：预测兑现、回执同源、两条拒绝路径没出门、曲线那条与回执同一口径（effective=true ⇒ ⚠ 已撤）");
process.exit(0);
