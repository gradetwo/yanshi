#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **判据：历史卡（`#history`）与撤销/重做在失败时，必须说清「哪条路 + 哪个码」** ✓。
//
// **抓的是哪一类缺口** ✓（第七轮效果卡 ⑩b/⑩d 的**同族第四、五格** ✓）：
//   ① **历史卡那一格** ✗：`refreshHistory()` 的失败分支只念
//        `读取失败：` + `value.error_code || "unknown"`
//      ⇒ 与服务端回执**同卡不同话** ✗ —— 同一份回执在效果卡上带责任归属（`读取失败（服务端）：…` ✓），
//        在历史卡上却是**裸码** ✓ ⇒ 用户分不清"服务端读不到"还是"本机没这端点" ✓；
//   ② **撤销/重做那两句** ✗（`:4456` / `:4482` 附近）：同样只读 `error_code` ✓
//      ⇒ 回执若把码放在 `error`（本地那一臂的格子名 ✓）⇒ 日志念成「撤销失败：unknown」✗
//      —— **页面自己把答案丢了** ✓（`failureCode()` 两个格子都读 ✓，同一文件里就有 ✓）。
//   **⇒ 本判据喂的是"真的失败回执"** ✓ 不是文案抽查 ✗：在**任何页面脚本之前**给
//      `window.fetch` 包一层桩（`Page.addScriptToEvaluateOnNewDocument` ✓，与
//      `browser-layer-lock` 同一条手法 ✓ —— 第 6 轮实测 `Fetch.enable` **一条都拦不到** ✗），
//      只拦三个工具：`get_log`／`undo_last`／`redo_last` ✓，其余原样转发 ✓。
//      **桩自己也被检查** ✓：命中次数当**前提** ⇒ 一次没接到 ⇒ `exit(3)` ✓ 不冒充产品绿 ✗。
//
// **钉住的八件事**（⓪~⑦ ✓）：
//   ⓪ 前提：桩注入成功且**真的接到** `get_log` ✓；历史卡在「历史」tab 里**可见** ✓
//      （折叠卡里的字对用户等于没说 ✗）；撤销按钮**可用**（夹具那一笔真在服务端算数 ✓）
//      ⇒ 拿不到任何一条 ⇒ 判据自身失效 ⇒ `exit(3)` ✓；
//   ① `get_log` 失败 ⇒ 卡上那句**逐字**是 `读取失败（服务端）：intercepted_read_failure` ✓
//      —— 带 venue ✓、带**具体码** ✓；红的就是现在那句 `读取失败：intercepted_read_failure` ✗
//      （同一句话在**本地那一臂**也得跟着变 ⇒ 这是 `refusalVenue()` 的活 ✓ 不是写死"服务端" ✓）；
//   ② 那句里**不许出现** `unknown` ✓（回执明明给了码 ✓）；
//   ③ 撤销失败（回执把码放在 `error` ✓ 本地那一臂的形状 ✓）⇒ `#log` 某行含
//      `撤销失败：intercepted_undo_failure` ✓ 且全日志**不含** `撤销失败：unknown` ✗；
//   ④ 同一颗按钮在**不注入**时**真的能撤**（撤销成功那行 + 判据自己问 `get_undo_status`
//      数字 1→0 ✓）⇒ 证明 ③ 测的就是那条真路径 ✓ 不是给按钮伪造的失败 ✓；
//   ⑤ 重做失败同理（`重做失败：intercepted_redo_failure` ✓ 不含 `unknown` ✗）；
//   ⑥ 撤掉注入 ⇒ 点「重新读取」⇒ 列表**真有行** ✓ 卡上那句话**退场** ✓（失败态不是钉子 ✗）；
//   ⑦ 英文模式 ⇒ **整句**是 `Read failed (the server): intercepted_read_failure` ✓
//      且**零汉字** ✓（`m40` 词条 `viewer-app.js:14086` 已在 ✓ 本轮只借它 ✓ 不新增；
//      `m19` 的裸句 `Read failed: …` 正是红的那一半 ⇒ ① 与 ⑦ 合起来把"venue 不许丢"钉死 ✓）。
//
// **口径**：导航 URL 钉死 `render=server` ✓（本条测的是走服务端那条路 ✓）；自建文档 ✓
// （夹具＝一层 ＋ 一块填充 ⇒ 历史里有原子、撤销计数＝1 ✓，跑完不碰别的文档 ✓）；
// 前提取不到 ⇒ `exit(3)` ✓（AGENTS.md：环境问题不许判成产品红 ✓）。
//
// 用法：node scripts/browser-history-failure-venue.mjs <viewer-url> [cdp-port]
import { setTimeout as sleep } from "node:timers/promises";

const judgeGuard = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 240000);

const url = process.argv[2];
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
if (!url) {
  console.error("用法: node scripts/browser-history-failure-venue.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const base = new URL(url).origin;
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};
const premiseFail = (why, detail) => {
  console.error("❌ 判据前提失败：" + why + (detail === undefined ? "" : " ⇒ " + JSON.stringify(detail).slice(0, 300)));
  process.exit(3);
};

// ---------- 夹具：自建文档（历史有原子 + 撤销计数为 1）----------
const DOC_W = 900;
const DOC_H = 600;
const docId = "t_hist_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: DOC_W, height: DOC_H }),
}).then((r) => r.json()).catch(() => null);
if (!opened || !opened.ok) premiseFail("开文档", opened);
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());
const LAYER = "HIST";
const madeLayer = await callTool("create_layer", { layer_id: LAYER, name: "history-fixture" });
if (!madeLayer.ok) premiseFail("建图层", madeLayer);
const filled = await callTool("fill_region", {
  layer_id: LAYER,
  shape: { type: "rect", x: 200, y: 150, w: 60, h: 60 },
  color: { r: 30, g: 120, b: 200, a: 255 },
});
if (!filled.ok) premiseFail("铺一块内容（撤销计数要它）", filled);
const undoBefore = await callTool("get_undo_status", {});
if (!undoBefore.ok || Number(undoBefore.remaining_gestures) !== 1) {
  premiseFail("夹具没有形成恰好一笔可撤销的笔迹", undoBefore);
}
ok("夹具就位：文档 " + docId + " 一层一块填充 ⇒ 服务端报可撤销 1 笔 ✓", { remaining: undoBefore.remaining_gestures });

// ---------- CDP ----------
const browser = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json().catch(() => null);
if (!browser || !browser.webSocketDebuggerUrl) premiseFail("CDP 在 " + port + " 上没给出 webSocketDebuggerUrl（浏览器没起来？）");
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
const targetId = created.result.targetId;
const send = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
await send("Runtime.enable");
await send("Page.enable");
await send("Log.enable");
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}

// ---------- 注入桩：只拦 get_log / undo_last / redo_last ----------
// **为什么拦在 `window.fetch`** ✓：`callTool` → `fetchOrLocal` → `fetch` 是产品的真路径 ✓
// （`fetchOrLocal` 里只读工具会做"网络优先 + 本地兜底"✓ 而桩回的是**成功的 HTTP 200** ⇒
//  走进它自己的 `response.json()` ⇒ **它就是一次普通的工具失败** ✓ 与真实服务端回执同形 ✓）。
// **`get_log` 的失败开关跨重载** ✓：英文那一臂要重新导航一次 ✗ ⇒ 桩从 `localStorage` 记的
// `"0"/"1"` 里读 ✓（缺省＝失败 ✓）⇒ 恢复那一臂改成 `"0"` 即可 ✓ 不必拆桩重装 ✓。
const KEY = "yanshiJudgeFailGetLog";
const STUB_SOURCE = `(() => {
  const KEY = ${JSON.stringify(KEY)};
  const state = {
    hits: { get_log: 0, undo_last: 0, redo_last: 0 },
    failGetLog: localStorage.getItem(KEY) !== "0",
    failUndo: false,
    failRedo: false,
  };
  window.__judgeStub = state;
  const real = window.fetch.bind(window);
  const reply = (body) => new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
  window.fetch = function (input, init) {
    try {
      const href = String(typeof input === "string" ? input : (input && input.url) || "");
      const method = String((init && init.method) || (input && input.method) || "GET").toUpperCase();
      if (method === "POST") {
        for (const tool of ["get_log", "undo_last", "redo_last"]) {
          if (href.indexOf("/api/tools/" + tool) >= 0) {
            state.hits[tool] += 1;
            if (tool === "get_log" && state.failGetLog) {
              return Promise.resolve(reply({ ok: false, error_code: "intercepted_read_failure" }));
            }
            if (tool === "undo_last" && state.failUndo) {
              return Promise.resolve(reply({ ok: false, error: "intercepted_undo_failure" }));
            }
            if (tool === "redo_last" && state.failRedo) {
              return Promise.resolve(reply({ ok: false, error: "intercepted_redo_failure" }));
            }
          }
        }
      }
    } catch (error) { /* 桩自己坏了 ⇒ 落回真实 fetch ⇒ 前提会红 */ }
    return real(input, init);
  };
})();`;
const stubAdded = await send("Page.addScriptToEvaluateOnNewDocument", { source: STUB_SOURCE });
const stubId = stubAdded && stubAdded.result ? stubAdded.result.identifier : null;
if (stubId === null) premiseFail("桩注入失败（addScriptToEvaluateOnNewDocument 没回 identifier）");

// ---------- 导航 + 等历史卡的那句话 ----------
const READ = `(() => {
  const stub = window.__judgeStub || null;
  const list = document.getElementById("history");
  return {
    stub: stub ? { hits: Object.assign({}, stub.hits), failGetLog: stub.failGetLog } : null,
    cell: list ? String(list.textContent || "") : null,
    rows: list ? list.querySelectorAll(".row").length : null,
    cellVisible: !!(list && list.offsetParent !== null),
    lang: document.documentElement.lang,
    log: (function () { const el = document.getElementById("log"); return el ? String(el.textContent || "") : null; })(),
  };
})()`;
async function openPage(lang) {
  await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=${lang}&render=server&v=${Date.now()}` });
  await waitEvent("Page.loadEventFired").catch(() => undefined);
  let read = null;
  for (let i = 0; i < 160; i += 1) {
    try { read = await evaluate(READ); } catch (_) { read = null; }
    const hit = read && read.stub && read.stub.hits.get_log > 0;
    const settled = read && (read.rows > 0 || /读取失败|Read failed/.test(String(read.cell || "")));
    if (hit && settled) break;
    await sleep(250);
  }
  return read;
}
const showHistory = () => evaluate(`(() => {
  const tab = document.querySelector('button[data-tab="history"]');
  if (tab) tab.click();
  for (const card of document.querySelectorAll('[data-panel="history"]')) card.classList.remove("collapsed");
  const list = document.getElementById("history");
  return { tab: !!tab, visible: !!(list && list.offsetParent !== null), cell: list ? String(list.textContent || "") : null };
})()`);

const zh = await openPage("zh");
if (!zh || !zh.stub) premiseFail("桩没装进页面（window.__judgeStub 不存在）", zh && zh.stub);
if (!(zh.stub.hits.get_log > 0)) premiseFail("桩一次都没接到 POST /api/tools/get_log", zh.stub.hits);
const cardZh = await showHistory();
if (!cardZh.visible) premiseFail("历史卡在「历史」tab 里不可见 ⇒ 用户看不到那句话 ⇒ 测的就不是屏幕上的事实", cardZh);
ok("前提：桩接到 get_log " + zh.stub.hits.get_log + " 次 ✓ 历史卡可见 ✓（历史 tab 已点开）", { cell: cardZh.cell });

const ZH_FAILED = "读取失败（服务端）：intercepted_read_failure";
const zhCell = String(cardZh.cell || "").trim();
if (zhCell !== ZH_FAILED) {
  bad("① `get_log` 失败时历史卡没有说清「哪条路 + 哪个码」", zhCell);
} else {
  ok("① 历史卡逐字是「" + ZH_FAILED + "」✓（venue 与具体码都在）");
}
if (/^读取失败：/.test(zhCell) || /unknown/.test(zhCell)) {
  bad("② 历史卡仍是裸码那一族（`读取失败：…` 或含 `unknown`）", zhCell);
} else {
  ok("② 卡上那句不带 `unknown`、也不是旧的裸码句式 ✓");
}

// ---------- ③~⑤ 撤销/重做：同一颗真按钮 ----------
const undoState = await evaluate(`(() => {
  const undo = document.querySelector('button[data-tool="undo"]');
  const redo = document.querySelector('button[data-tool="redo"]');
  return {
    undoDisabled: undo ? undo.disabled === true : null,
    redoDisabled: redo ? redo.disabled === true : null,
    depth: (() => { const el = document.getElementById("undoDepth"); return el ? String(el.textContent || "") : null; })(),
  };
})()`);
if (undoState.undoDisabled !== false) {
  premiseFail("撤销按钮是禁用的 ⇒ 夹具那一笔没被页面当成可撤销（环境/夹具问题，不是产品红）", undoState);
}
const clickTool = (tool) => evaluate(`(() => {
  const b = document.querySelector('button[data-tool="${tool}"]');
  if (!b) return "missing";
  b.click();
  return "clicked";
})()`);
const logText = () => evaluate(`(() => {
  const el = document.getElementById("log");
  return el ? String(el.textContent || "") : null;
})()`);
async function waitFor(regex, tries = 80) {
  for (let i = 0; i < tries; i += 1) {
    const text = await logText();
    if (text && regex.test(text)) return text;
    await sleep(250);
  }
  return await logText();
}

// ③ 撤销失败：回执把码放在 `error`（本地那一臂的格子名）⇒ 页面必须念出来 ✓
await evaluate(`(() => { window.__judgeStub.failUndo = true; return 1; })()`);
const undoClick = await clickTool("undo");
if (undoClick !== "clicked") premiseFail("找不到撤销按钮");
const afterUndoFail = await waitFor(/撤销失败：/);
const undoHits = await evaluate(`window.__judgeStub.hits.undo_last`);
if (!(undoHits > 0)) premiseFail("撤销那一下没走到桩（undo_last 命中 0）⇒ 点的不是那次调用", { undoHits });
const undoFailLine = /撤销失败：(intercepted_undo_failure|unknown)/.exec(String(afterUndoFail || ""));
if (!undoFailLine) {
  bad("③ 撤销失败的日志行没出现（或形状变了）", String(afterUndoFail || "").slice(-200));
} else if (undoFailLine[1] !== "intercepted_undo_failure") {
  bad("③ 撤销失败把回执里的码丢了（回执给的是 `error` 格子）", undoFailLine[0]);
} else {
  ok("③ 撤销失败念的是「撤销失败：intercepted_undo_failure」✓（`error` 格子也读）");
}
if (/撤销失败：unknown/.test(String(afterUndoFail || ""))) {
  bad("③ 全日志里仍有「撤销失败：unknown」", String(afterUndoFail || "").slice(-200));
}

// ④ 同一颗按钮在**不注入**时真的能撤 ⇒ ③ 测的就是真路径 ✓（判据自己问服务端要数字 ✓）
await evaluate(`(() => { window.__judgeStub.failUndo = false; return 1; })()`);
await clickTool("undo");
const afterUndoReal = await waitFor(/已撤销 \d+ 笔/);
const undoAfter = await callTool("get_undo_status", {});
if (!/已撤销 \d+ 笔/.test(String(afterUndoReal || ""))) {
  bad("④ 撤掉注入后撤销没有成功（夹具/环境问题或产品真坏了）", String(afterUndoReal || "").slice(-200));
} else if (!undoAfter.ok || Number(undoAfter.remaining_gestures) !== 0) {
  bad("④ 页面说撤销了，服务端却说还能撤 ⇒ 界面与事实不一致", undoAfter);
} else {
  ok("④ 真撤销成功：页面说「已撤销…」✓ 服务端 remaining 1→0 ✓（③ 的失败是同一颗按钮）", { remaining: undoAfter.remaining_gestures });
}

// ⑤ 重做失败：码同样只在 `error` 里
await evaluate(`(() => { window.__judgeStub.failRedo = true; return 1; })()`);
await sleep(300);
const redoClick = await clickTool("redo");
if (redoClick !== "clicked") premiseFail("找不到重做按钮");
const afterRedoFail = await waitFor(/重做失败：/);
const redoHits = await evaluate(`window.__judgeStub.hits.redo_last`);
if (!(redoHits > 0)) premiseFail("重做那一下没走到桩（redo_last 命中 0）", { redoHits });
const redoFailLine = /重做失败：(intercepted_redo_failure|unknown)/.exec(String(afterRedoFail || ""));
if (!redoFailLine) {
  bad("⑤ 重做失败的日志行没出现（或形状变了）", String(afterRedoFail || "").slice(-200));
} else if (redoFailLine[1] !== "intercepted_redo_failure") {
  bad("⑤ 重做失败把回执里的码丢了", redoFailLine[0]);
} else {
  ok("⑤ 重做失败念的是「重做失败：intercepted_redo_failure」✓");
}

// ---------- ⑥ 撤掉注入 ⇒ 列表真的回来 ----------
await evaluate(`(() => {
  localStorage.setItem(${JSON.stringify(KEY)}, "0");
  window.__judgeStub.failGetLog = false;
  const b = document.getElementById("historyReload");
  if (b) b.click();
  return 1;
})()`);
let recovered = null;
for (let i = 0; i < 120; i += 1) {
  recovered = await evaluate(READ);
  if (recovered && recovered.rows > 0 && !/读取失败/.test(String(recovered.cell || ""))) break;
  await sleep(250);
}
if (!recovered || !(recovered.rows > 0)) {
  bad("⑥ 撤掉注入后历史列表没有行（「重新读取」没把列表拉回来）", recovered && { rows: recovered.rows, cell: recovered.cell });
} else if (/读取失败/.test(String(recovered.cell || ""))) {
  bad("⑥ 列表回来了但卡上那句失败还钉着", recovered.cell);
} else {
  ok("⑥ 撤掉注入 ⇒ 「重新读取」后列表有 " + recovered.rows + " 行 ✓ 失败那句退场 ✓");
}

// ---------- ⑦ 英文模式 ----------
await evaluate(`localStorage.setItem(${JSON.stringify(KEY)}, "1")`);
const en = await openPage("en");
if (!en || !en.stub) premiseFail("英文那一页没装上桩");
if (!(en.stub.hits.get_log > 0)) premiseFail("英文那一页桩没接到 get_log", en.stub.hits);
const cardEn = await showHistory();
if (!cardEn.visible) premiseFail("英文页历史卡不可见", cardEn);
const EN_FAILED = "Read failed (the server): intercepted_read_failure";
const enCell = String(cardEn.cell || "").trim();
if (enCell !== EN_FAILED) {
  bad("⑦ 英文模式整句不是「" + EN_FAILED + "」", enCell);
} else {
  ok("⑦ 英文模式整句是「" + EN_FAILED + "」✓（`m40` 词条照旧 ✓ 没新增翻译）");
}
if (/[\u4e00-\u9fff]/.test(enCell)) {
  bad("⑦ 英文模式那句里还有汉字", enCell);
} else {
  ok("⑦ 英文模式那句零汉字 ✓");
}

// ---------- 收尾 ----------
await evaluate(`(() => { try { localStorage.removeItem(${JSON.stringify(KEY)}); } catch (_) {} return 1; })()`).catch(() => undefined);
if (pageErrors.length > 0) {
  bad("⑧ 页面有 JS 异常（判据跑的这条路上不许有）", pageErrors.slice(0, 4));
} else {
  ok("⑧ 全程 0 条 JS 异常 ✓");
}
await sendB("Target.closeTarget", { targetId }).catch(() => undefined);

console.log("\n=== 汇总 ===");
if (problems.length === 0) {
  console.log("  历史卡与撤销/重做都把「哪条路 + 哪个码」说清了 ✓ 中英两种整句逐字对得上 ✓");
  console.log("  失败态会退场（重新读取后有行 ✓）✗ 没有把一次失败钉成常态 ✓");
  clearTimeout(judgeGuard);
  process.exit(0);
}
console.log("  ✗ " + problems.length + " 条：");
for (const p of problems) console.log("    - " + p);
clearTimeout(judgeGuard);
process.exit(1);
