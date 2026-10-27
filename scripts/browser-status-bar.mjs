#!/usr/bin/env node
// **判据：顶栏的 `head / rendered / dirty` 三个数必须自己收敛到服务端的事实** ✓。
//
// **抓的是哪一类缺口** ✓：本轮是**看整页截图**才发现的（不是读代码读出来的 ✗）：
// 中英两页都写着 `head 25 / rendered 22 / dirty 0 / 已保存` ⇒ 用户读到的是
// **"画布还差 3 个原子没画"**, 而 `dirty 0` 与 `已保存` 又说它画完了 ✓ 三个数互相打脸 ✓
// 而那个 `22` **永远不会自己变** ✓。两条独立成因（都在 `viewer-app.js` ✓ 本轮都修了 ✓）：
//   A `refreshThumb()` 是 `#rendered` 唯一的**服务端来源**写者 ✓ 而它每次改动只读**一回**
//     （400 ms 去抖 ✓）⇒ 正好读在服务端水位还没追上 head 的那一瞬 ✗ 之后无人重读 ✓ 数字搁死；
//   B **经 WS 到达的原子**（别的客户端 ✓ 或自动化直接打 HTTP ✓）只推 `head` ✓
//     压根没有人去重读水位 ✓ ⇒ 实测开局 `head 1 / rendered 0` ⇒ 远端连建三层
//     ⇒ 顶栏停在 `4 / 0 / 0` 六秒不动 ✗ 而同一时刻服务端的 `rendered_seq` 早就等于 `head_seq` 了 ✓
//     ⇒ **纯粹是页面没再读** ✓ 不是服务端算了错 ✓。
//
// **钉住的六件事**：
//   ① 新文档载入 ⇒ 顶栏 `rendered` 必须在有界时间内**追平** `head`（不是靠手工刷新页面 ✓）；
//   ② 轨迹两保：全程 `rendered <= head`（水位不许跑到 head 前面 ✓ 那同样是假数 ✓）
//      且 `head` 不降时 `rendered` 不许倒退 ✓（乐观写与去抖重读来回打摆 ✗ 用户看到的数字会跳）；
//   ③ **远端**连建 3 层 ⇒ `head` 跟到 4 ✓ 且 `rendered` 同样在有界时间内追平（守卫成因 B ✓）；
//   ④ 追平那一刻，页面三个数与**同一时刻**服务端 `get_document` 的 `head_seq / rendered_seq`
//      逐位相等 ⇒ 界面不许留第二个真值 ✓（与本轮 `browser-dodge-burn.mjs` 同一口径 ✓）；
//   ⑤ 英文页同样跑 ①③④ ✓；
//   ⑥ 全程 0 条 JS 错误 ✓。
//
// **变异实测**（本轮两处各自单独抽掉、重编译、重跑 ✓ 不是推理 ✓）：
//   抽掉 A（WS 原子事件里那次 `scheduleThumbRefresh()`）⇒ 本判据 ⑤ 红
//     （远端建一层之后 `head 2 / rendered 1 / dirty 0` 停死 9 秒 ✗）；
//   抽掉 B（读到落后时再排一次的那个上限 6 次的循环）⇒ 本判据 ①⑤ 红
//     （开局 `head 1 / rendered 0` 停死 9 秒 ✗ 45 次读数一次没变）⇒ **两条各自都被独立钉住** ✓
//   另：服务端**有时**会广播 `thumbnail` 事件替页面重读 ✓ 但那是"恰好撞上"✗
//     同一次变异下三层的用例碰巧绿、一层的用例红 ⇒ **不能靠它** ✓ 页面自己要保证收敛 ✓。
//
// **口径**：本条测的是**服务端水位**与顶栏显示 ⇒ 钉死 `render=server` ✓
//   （`run-criteria.sh` 对 browser-* 统一注入 `&render=client` ✗ 本地内核不去读服务端像素 ✓
//   水位就永远不动 ✓ 那是测本地缺端点 ✗ 已把本条加进那条例外名单 ✓）。
// 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)` ✓（AGENTS.md：不许把环境问题判成产品红 ✓）。
// 用法：node scripts/browser-status-bar.mjs <viewer-url> [cdp-port]
import { setTimeout as sleep } from "node:timers/promises";

const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 150000);

const url = process.argv[2];
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
const base = url ? new URL(url).origin : "http://127.0.0.1:15900";
if (!url) { console.error("用法: node scripts/browser-status-bar.mjs <viewer-url> [cdp-port]"); process.exit(2); }
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => { problems.push(label); console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]")); };

// ---------- 两份干净文档（各自独立 ⇒ 断言互不干扰 ✓） ----------
const newDoc = async () => {
  const docId = "t_sb_" + Date.now().toString(36).slice(-6) + Math.floor(Math.random() * 9);
  const opened = await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width: 160, height: 120 }),
  }).then((r) => r.json());
  if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
  return { docId, token: opened.token };
}
const docZh = await newDoc();
const docEn = await newDoc();
const serverNums = async (doc) => {
  const v = await fetch(`${base}/api/tools/get_document?doc=${encodeURIComponent(doc.docId)}&token=${encodeURIComponent(doc.token)}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: "{}" }).then((r) => r.json());
  if (v.head_seq === undefined || v.rendered_seq === undefined) {
    console.error("❌ 判据前提失败：服务端 get_document 没给 head_seq/rendered_seq ⇒ " + JSON.stringify(v).slice(0, 200));
    process.exit(3);
  }
  return { head: Number(v.head_seq), rendered: Number(v.rendered_seq) };
};

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
const BAR = `(() => { const g = (i) => (document.getElementById(i) || {}).textContent;
  return { head: Number(g("head")), rendered: Number(g("rendered")), dirty: Number(g("dirty")) }; })()`;
const readBar = async () => evaluate(BAR);
/** 擦掉可能残留的本地渲染偏好 ✓（`localStorage` 按源共享 ✗ 见 `browser-dodge-burn.mjs` 同处教训 ✓）。 */
async function openViewer(doc, lang) {
  await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
  await waitEvent("Page.loadEventFired");
  await sleep(300);
  await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); return 1; })()`);
  await send("Page.navigate", { url: `${base}/?doc=${doc.docId}&token=${doc.token}&lang=${lang}&render=server&v=${Date.now()}` });
  await waitEvent("Page.loadEventFired");
  for (let i = 0; i < 40; i += 1) {
    const bar = await readBar();
    if (Number.isFinite(bar.head) && bar.head >= 1) return bar;
    await sleep(250);
  }
  console.error("❌ 判据前提失败：载入后顶栏 `head` 一直是空/0（lang=" + lang + "）");
  process.exit(3);
}
/** **有界等待"这一轮真的结束"** ✓：`head` 先跟到 `expectHead`（WS 到达有延迟 ✓ 不能一上来就比 ✓
 *  否则"追平"会被**还没发生的那次改动**判成 vacuous ✓ 本轮自己就红过一次 ✗）
 *  再要求 `rendered === head` ✓ 轨迹一并记下来（② 那条断言的根据 ✓ 单次读数看不见 ✗）。 */
async function settle(expectHead, ms = 9000) {
  const trail = [];
  const stop = Date.now() + ms;
  let bar = await readBar();
  trail.push(bar);
  while (Date.now() < stop) {
    if (bar.head === expectHead && bar.rendered === bar.head && bar.head >= 1) {
      await sleep(600); // 让在飞去抖那一读落地 ✓ 免得比完立刻被服务端数覆盖 ✓
      return { caught: true, bar: await readBar(), trail };
    }
    await sleep(200);
    bar = await readBar();
    trail.push(bar);
  }
  return { caught: false, bar, trail };
}
const checkTrail = (trail, label) => {
  let over, back;
  for (let i = 0; i < trail.length; i += 1) {
    const s = trail[i];
    if (!Number.isFinite(s.head) || !Number.isFinite(s.rendered)) continue;
    if (s.rendered > s.head && !over) over = `${s.head}/${s.rendered}`;
    if (i > 0) {
      const p = trail[i - 1];
      if (s.head >= p.head && p.head >= 1 && s.rendered < p.rendered && !back) {
        back = `${p.head}/${p.rendered} → ${s.head}/${s.rendered}`;
      }
    }
  }
  if (over) bad(label + "：出现 `rendered > head` 的假数", over);
  else ok(label + "：全程 `rendered <= head`", trail.length + " 次读数");
  if (back) bad(label + "：`head` 没降而 `rendered` 倒退（数字来回打摆）", back);
  else ok(label + "：`head` 不降时 `rendered` 不倒退");
};

// ---------- 中文页 ----------
const bar0 = await openViewer(docZh, "zh");
ok("开局：顶栏读到数字", bar0);
const w1 = await settle(bar0.head);
if (!w1.caught) bad("① 新文档载入后 `rendered` 必须在 9 秒内追平 `head`（本轮实测搁死在 `1 / 0` ✗）", w1.bar);
else ok("① 载入后 `rendered` 自己追平 `head`", { head: w1.bar.head, rendered: w1.bar.rendered });
checkTrail(w1.trail, "②/①");

// 远端连建三层（自动化直打 HTTP ⇒ 页面只能经 WS 事件知道 ✓ 这正是成因 B 那条路 ✓）
for (const id of ["RA", "RB", "RC"]) {
  const r = await fetch(`${base}/api/tools/create_layer?doc=${docZh.docId}&token=${docZh.token}`,
    { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ layer_id: id, name: id }) }).then((x) => x.json());
  if (!r.ok) { console.error("❌ 判据前提失败：远端建层 ⇒ " + JSON.stringify(r).slice(0, 200)); process.exit(3); }
}
const wantZh = bar0.head + 3;
const srvZh = await serverNums(docZh);
if (srvZh.head !== wantZh) { console.error("❌ 判据前提失败：服务端 head 不是 " + wantZh + " ⇒ " + JSON.stringify(srvZh)); process.exit(3); }
const w2 = await settle(wantZh);
if (w2.bar.head !== wantZh) bad("③ 远端建了 3 层，顶栏 `head` 没跟上来", w2.bar);
else ok("③ 远端原子把 `head` 推上去了", w2.bar);
if (!w2.caught) bad("③ 远端原子之后 `rendered` 必须自己追平（本轮实测停在 `4 / 0 / 0` 六秒不动 ✗）", w2.bar);
else ok("③ 远端原子后 `rendered` 自己追平", w2.bar);
checkTrail(w2.trail, "②/③");

// 同一时刻服务端的数（④：不许有第二个真值 ✓）
const srvZh2 = await serverNums(docZh);
const atZh = w2.bar;
if (!(atZh.head === srvZh2.head && atZh.rendered === srvZh2.rendered)) {
  bad("④ 页面三个数与同一时刻服务端的数不等（界面自己留了第二个真值 ✗）", { page: atZh, server: srvZh2 });
} else ok("④ 页面数字 == 同一时刻服务端数字", { page: `${atZh.head}/${atZh.rendered}`, server: `${srvZh2.head}/${srvZh2.rendered}` });

// ---------- 英文页 ----------
const barEn = await openViewer(docEn, "en");
ok("⑤ 英文页：顶栏读到数字", barEn);
const w3 = await settle(barEn.head);
if (!w3.caught) bad("⑤ 英文页载入后 `rendered` 没追平 `head`", w3.bar);
else ok("⑤ 英文页载入后 `rendered` 自己追平 `head`", w3.bar);
checkTrail(w3.trail, "②/⑤");
await fetch(`${base}/api/tools/create_layer?doc=${docEn.docId}&token=${docEn.token}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ layer_id: "EA", name: "EA" }) }).then((x) => x.json());
const wantEn = barEn.head + 1;
const w4 = await settle(wantEn);
if (w4.bar.head !== wantEn) bad("⑤ 英文页远端原子后顶栏 `head` 没跟上来", w4.bar);
if (!w4.caught) bad("⑤ 英文页远端原子后 `rendered` 没追平", w4.bar);
else ok("⑤ 英文页远端原子后 `rendered` 自己追平", w4.bar);
checkTrail(w4.trail, "②/⑤");
const srvEn = await serverNums(docEn);
const atEn = w4.bar;
if (!(atEn.head === srvEn.head && atEn.rendered === srvEn.rendered)) bad("④ 英文页数字与服务端不等", { page: atEn, server: srvEn });
else ok("④ 英文页数字 == 同一时刻服务端数字", { page: `${atEn.head}/${atEn.rendered}`, server: `${srvEn.head}/${srvEn.rendered}` });

if (pageErrors.length) bad("⑥ 全程 0 条 JS 错误", pageErrors.slice(0, 3));
else ok("⑥ 全程 0 条 JS 错误");

ws.close();
clearTimeout(__judgeTimeout);
if (problems.length) {
  console.log("❌ 顶栏水位判据 " + problems.length + " 条红：");
  for (const p of problems) console.log("   - " + p);
  process.exit(1);
}
console.log("✅ 顶栏 `head/rendered/dirty` 自己收敛、且与服务端同数 ✓");
