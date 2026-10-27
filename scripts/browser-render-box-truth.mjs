#!/usr/bin/env node
// **判据：「用服务端渲染」勾选框在首帧就必须说真话** ✓（第九轮 ✓）。
//
// **抓的是哪一类缺口** ✓：`#useServerRender` 的初值原本只在 `setupFileMenu()` 里赋值 ✓
// （`viewer-app.js` 的 `useServerRenderBox.checked = serverRenderPreferred()` ✓），
// 而那条链是 `boot()` → `warmKernel()` → `setupAssetPanels()` ✓ ⇒ **要等启动尾部的网络** ✗
// ⇒ **∴ 默认（服务端渲染 ✓）时首帧画的是**空框 ✗ 而标签写着"默认开启" ✓
// ⇒ **屏幕自相矛盾** ✗（实测：首绘 52 ms 时 `checked=false` ✓，≈230 ms 才变 true ✓；
//   慢网／慢设备下这段窗口按启动尾部的时间拉长 ✗）。
//
// **修法** ✓：模板在 `#useServerRender` 之后**同步**定下初值 ✓（`viewer.rs` 内联脚本 ✓），
// 三档顺序与 `serverRenderPreferred()` 逐档一致 ✓（本地专用／URL `?render`／存量偏好／缺省开 ✓）。
// **∴ 本判据**把"首帧"做成**可观察的事实** ✗：在**任何页面脚本之前**装记录器
// （`Page.addScriptToEvaluateOnNewDocument` ✓；第 6 轮实测 `Fetch.enable` 拦不到 ✗），
// 在 `DOMContentLoaded` 与**之后第一帧 `requestAnimationFrame`** 各读一次勾选框 ✓。
//
// **钉住的六件事**（⓪~⑤ ✓）：
//   ⓪ 前提：记录器装进页面 ✓、勾选框在 DCL 时**存在且可见** ✓、启动真的走到
//      `setupFileMenu()`（`#fileMenuBody` 有子节点 ✓）⇒ 任何一条拿不到 ⇒ `exit(3)` ✓；
//   ① 缺省（无参数、无偏好）⇒ DCL 时**已勾上** ✓（红的就是现在这格：`false` ✗）；
//   ② 存量偏好 `"1"` ⇒ DCL 已勾上 ✓（同一族 ✓）；
//   ③ `?render=server` 压过存量 `"0"` ⇒ DCL 已勾上 ✓；
//   ④ `?render=client` 压过存量 `"1"` ⇒ DCL **未勾** ✓（**修复不许把这一档也说成服务端** ✗）；
//   ⑤ 存量偏好 `"0"` ⇒ DCL 未勾 ✓；并且每一档**启动后**的勾选状态仍等于首帧 ✓
//      （`setupFileMenu()` 的二次对账不许把首帧的答案翻掉 ✗）。
//   ⑥ 本地专用（`__pwaLocalOnly = true` ✓，存量 `"1"` 也不许翻 ✓）⇒ DCL 未勾 ✓、
//      启动后**禁用** ✓、勾选框**仍然存在** ✓（红的是现在这格：说明文字用
//      `label.textContent = …` 写 ⇒ **把勾选框节点一起抹掉** ✗ ⇒ 界面上只剩一句话 ✓）、
//      且"（本部署无服务端，不可用）"的说明**可见** ✓。
//
// **口径**：五档逐档导航（同一个自建文档 ✓ `t_box_<ts>` ✓）；每档在 `document-start`
// 里**播种** `localStorage["yanshi.serverRender"]` ✓（清／"0"／"1" ✓），跑完**还原**原值 ✓
// （`browser-render-switch` 的教训：localStorage 按源共享 ✗）。
// 前提取不到 ⇒ `exit(3)` ✓；产品不对 ⇒ `exit(1)` ✓。
//
// 用法：node scripts/browser-render-box-truth.mjs <viewer-url> [base] [token] [cdp-port]
import { setTimeout as sleep } from "node:timers/promises";

const judgeGuard = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 240000);

const url = process.argv[2];
if (!url) {
  console.error("用法: node scripts/browser-render-box-truth.mjs <viewer-url> [base] [token] [cdp-port]");
  process.exit(2);
}
const argBase = /^https?:\/\//.test(process.argv[3] || "") ? process.argv[3] : null;
const base = (argBase || new URL(url).origin).replace(/\/$/, "");
const port = (/^\d+$/.test(process.argv[5] || "") && process.argv[5]) ||
  (/^\d+$/.test(process.argv[3] || "") && process.argv[3]) ||
  process.env.CDP_PORT || "9333";

const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};
const premiseFail = (why, detail) => {
  console.error("❌ 判据前提失败：" + why + (detail === undefined ? "" : " ⇒ " + JSON.stringify(detail).slice(0, 400)));
  process.exit(3);
};

const STORAGE_KEY = "yanshi.serverRender";

// ---------- 夹具：自建文档（不碰别的文档 ✓）----------
const docId = "t_box_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 900, height: 600 }),
}).then((r) => r.json()).catch(() => null);
if (!opened || !opened.ok) premiseFail("开文档", opened);
const token = opened.token;
ok("夹具就位：文档 " + docId + " ✓");

// ---------- CDP ----------
const browser = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json().catch(() => null);
if (!browser || !browser.webSocketDebuggerUrl) premiseFail("CDP 在 " + port + " 上没给出 webSocketDebuggerUrl（浏览器没起来？）");
const ws = new WebSocket(browser.webSocketDebuggerUrl);
let msgId = 0;
const pending = new Map();
const pageErrors = [];
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
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
if (!created.result || !created.result.targetId) premiseFail("Target.createTarget 没给 targetId", created);
const targetId = created.result.targetId;
const attached = await sendB("Target.attachToTarget", { targetId, flatten: true });
if (!attached.result || !attached.result.sessionId) premiseFail("Target.attachToTarget 没给 sessionId", attached);
const sessionId = attached.result.sessionId;
const send = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params, sessionId })); });
await send("Runtime.enable");
await send("Page.enable");
await send("Log.enable");
await send("Emulation.setDeviceMetricsOverride", { width: 1280, height: 860, deviceScaleFactor: 1, mobile: false });
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}

// ---------- 记录器：在任何页面脚本之前装（每次导航都重装 ✓）----------
const RECORDER = `(() => {
  const rec = window.__boxTruth = { events: [] };
  const snap = (why) => {
    const b = document.getElementById("useServerRender");
    rec.events.push({
      t: Math.round(performance.now()), why,
      exists: !!b,
      checked: b ? b.checked : null,
      disabled: b ? b.disabled : null,
      visible: b ? !!(b.offsetWidth || b.offsetHeight) : null,
      ready: document.readyState,
    });
  };
  snap("boot");
  document.addEventListener("DOMContentLoaded", () => {
    snap("dcl");
    let n = 0;
    const loop = () => { snap("raf" + (n++)); if (n < 12) requestAnimationFrame(loop); };
    requestAnimationFrame(loop);
  });
  try {
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) rec.events.push({ t: Math.round(e.startTime), why: "paint:" + e.name });
    }).observe({ type: "paint", buffered: true });
  } catch (error) { /* 没有 paint 时间线也不影响断言 ✓ */ }
})();`;
const recorderAdded = await send("Page.addScriptToEvaluateOnNewDocument", { source: RECORDER });
if (!recorderAdded.result || !recorderAdded.result.identifier) premiseFail("记录器注入失败（addScriptToEvaluateOnNewDocument 没回 identifier）", recorderAdded);

const seedSource = (value) => `(() => { try {
  const k = ${JSON.stringify(STORAGE_KEY)};
  const v = ${JSON.stringify(value)};
  if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v);
} catch (error) {} })();`;

const READ = `(() => {
  const b = document.getElementById("useServerRender");
  const label = b ? (b.closest("label") || b.parentElement) : null;
  const body = document.getElementById("fileMenuBody");
  return {
    ready: document.readyState,
    rec: window.__boxTruth ? window.__boxTruth.events : null,
    box: b ? { checked: b.checked, disabled: b.disabled, visible: !!(b.offsetWidth || b.offsetHeight) } : null,
    label: label ? String(label.textContent || "").trim() : null,
    fileMenuReady: !!(body && body.children.length > 0),
    wasmState: (document.getElementById("wasmState") || {}).textContent || null,
    hint: (() => {
      const hint = document.getElementById("serverRenderLocalOnlyHint");
      return hint ? { hidden: !!hint.hidden, visible: !!(hint.offsetWidth || hint.offsetHeight), text: String(hint.textContent || "").trim() } : null;
    })(),
    stored: (() => { try { return localStorage.getItem(${JSON.stringify(STORAGE_KEY)}); } catch (error) { return "err"; } })(),
    stats: window.yanshiStats ? {
      wasm: !!window.yanshiStats.wasm,
      serverCalls: window.yanshiStats.serverCalls,
      kernelLoadFailures: window.yanshiStats.kernelLoadFailures || 0,
    } : null,
  };
})()`;

async function waitFor(expression, timeoutMs) {
  const started = Date.now();
  let last = null;
  while (Date.now() - started < timeoutMs) {
    try { last = await evaluate(expression); } catch (_) { last = null; }
    if (last) return last;
    await sleep(200);
  }
  return last;
}

// ---------- 读出原始偏好（收尾要还原 ✓）----------
await send("Page.navigate", { url: `${base}/?v=setup-${Date.now()}` });
await waitFor(`document.readyState === "complete"`, 15000);
const originalStored = await evaluate(`(() => { try { return localStorage.getItem(${JSON.stringify(STORAGE_KEY)}); } catch (error) { return "err"; } })()`);
if (originalStored === undefined) premiseFail("读不到原始 localStorage 偏好");
ok("原始偏好已记下（收尾还原）", { [STORAGE_KEY]: originalStored });
pageErrors.length = 0;

// ---------- 六档 ----------
const cases = [
  { name: "缺省（无参数、无偏好）", seed: null, render: null, initial: true, path: "server" },
  { name: "存量偏好 \"1\"", seed: "1", render: null, initial: true, path: "server" },
  { name: "?render=server 压过存量 \"0\"", seed: "0", render: "server", initial: true, path: "server" },
  { name: "?render=client 压过存量 \"1\"", seed: "1", render: "client", initial: false, path: "client" },
  { name: "存量偏好 \"0\"", seed: "0", render: null, initial: false, path: "client" },
  { name: "本地专用 __pwaLocalOnly（存量 \"1\"）", seed: "1", render: null, initial: false, path: "local", localOnly: true, expectDisabled: true },
];

for (let index = 0; index < cases.length; index += 1) {
  const c = cases[index];
  const seed = await send("Page.addScriptToEvaluateOnNewDocument", { source: seedSource(c.seed) });
  const seedId = seed.result && seed.result.identifier;
  if (!seedId) premiseFail("播种脚本注入失败（第 " + (index + 1) + " 档）", seed);
  let localOnlyId = null;
  if (c.localOnly) {
    const sentinel = await send("Page.addScriptToEvaluateOnNewDocument", { source: `(() => { window.__pwaLocalOnly = true; })();` });
    localOnlyId = sentinel.result && sentinel.result.identifier;
    if (!localOnlyId) premiseFail("localOnly 哨兵注入失败（第 " + (index + 1) + " 档）", sentinel);
  }
  const errStart = pageErrors.length;

  const qs = new URLSearchParams({ doc: docId, token, lang: "zh", v: `case${index}-${Date.now()}` });
  if (c.render) qs.set("render", c.render);
  await send("Page.navigate", { url: `${base}/?${qs.toString()}` });

  // 等到：DCL 记录在、启动真的走到 setupFileMenu（勾选框的二次对账点 ✓）
  let read = null;
  const started = Date.now();
  while (Date.now() - started < 25000) {
    try { read = await evaluate(READ); } catch (_) { read = null; }
    if (read && read.rec && read.rec.some((e) => e.why === "dcl") && read.fileMenuReady) break;
    await sleep(250);
  }
  await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: seedId });
  if (localOnlyId) await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: localOnlyId });

  if (!read || !read.rec) premiseFail("记录器没装进页面（第 " + (index + 1) + " 档）", read);
  const dcl = read.rec.find((e) => e.why === "dcl");
  const firstRaf = read.rec.find((e) => e.why && e.why.startsWith("raf"));
  if (!dcl || !dcl.exists) premiseFail("DCL 时勾选框不存在（第 " + (index + 1) + " 档）", read.rec.slice(0, 6));
  if (!dcl.visible) premiseFail("DCL 时勾选框不可见（第 " + (index + 1) + " 档）", dcl);
  if (!read.fileMenuReady) premiseFail("启动没走到 setupFileMenu（第 " + (index + 1) + " 档，25 s）", { ready: read.ready, wasmState: read.wasmState });
  if (!firstRaf) premiseFail("DCL 之后没有 rAF 记录（第 " + (index + 1) + " 档）");

  const n = index + 1;
  const tag = "第 " + n + " 档 " + c.name;
  if (dcl.checked !== c.initial) {
    bad(`① ${tag}：DOMContentLoaded 时勾选框 = ${dcl.checked}，应当是 ${c.initial} ⇒ 首帧在说假话 ✗`, {
      checkedAtDcl: dcl.checked, checkedAtFirstRaf: firstRaf.checked, tDcl: dcl.t,
    });
  } else {
    ok(`① ${tag}：DCL 时勾选框 = ${dcl.checked} ✓（首帧已说真话）`, { tDcl: dcl.t });
  }
  if (firstRaf.checked !== c.initial) {
    bad(`② ${tag}：DCL 后第一帧勾选框 = ${firstRaf.checked}，应当是 ${c.initial} ✗`, { tRaf: firstRaf.t });
  } else {
    ok(`② ${tag}：DCL 后第一帧勾选框 = ${firstRaf.checked} ✓`);
  }

  // 两次对账之间有没有被改回去 / 改过头（修复不许把客户端那一档也说成服务端 ✗）
  const flips = [];
  let prev = null;
  for (const e of read.rec) {
    if (typeof e.checked !== "boolean" || !e.exists) continue;
    if (prev !== null && e.checked !== prev) flips.push({ from: prev, to: e.checked, t: e.t, why: e.why });
    prev = e.checked;
  }
  const finalExpected = read.stats && read.stats.kernelLoadFailures > 0 ? true : c.initial;
  if (c.expectDisabled) {
    // **本地专用那一档的红点** ✗：现在的产品用 `label.textContent = …` 写说明 ⇒
    // 把 `<input>` 节点**一起抹掉** ✗ ⇒ 启动后勾选框**不存在** ✓（截图里只剩一句话 ✓）。
    if (!read.box) {
      bad(`③ ${tag}：启动后勾选框**消失了** ✗（说明文字把节点一起抹掉）`, {
        hint: read.hint, label: read.label, flips, wasmState: read.wasmState,
      });
    } else if (read.box.checked !== false || !read.box.disabled) {
      bad(`③ ${tag}：启动后勾选框 = {checked:${read.box.checked}, disabled:${read.box.disabled}}，应当 {checked:false, disabled:true} ✗`, {
        flips, wasmState: read.wasmState,
      });
    } else {
      ok(`③ ${tag}：启动后勾选框仍在、未勾且禁用 ✓`);
    }
    if (!read.hint || !read.hint.visible || !read.hint.text) {
      bad(`④ ${tag}：没有显示"本部署无服务端，不可用"的说明 ✗`, read.hint);
    } else {
      ok(`④ ${tag}：说明可见 ✓`, read.hint.text);
    }
  } else if (!read.box) {
    bad(`③ ${tag}：启动后勾选框不见了 ✗`);
  } else if (read.box.checked !== finalExpected) {
    bad(`③ ${tag}：启动后勾选框 = ${read.box.checked}，应当是 ${finalExpected} ✗`, {
      flips, wasmState: read.wasmState, stats: read.stats,
    });
  } else if (read.stats && read.stats.kernelLoadFailures > 0) {
    console.log(`  ⚠️ ${tag}：本地内核装载失败 ${read.stats.kernelLoadFailures} 次 ⇒ 产品热回退到服务端 ✓ 勾选框 = ${read.box.checked}（按热回退对账 ✓，不算红）`);
  } else {
    ok(`③ ${tag}：启动后勾选框 = ${read.box.checked} ✓（${c.path} 路径）`);
  }
  if (!c.expectDisabled && read.box && read.box.disabled) {
    bad(`④ ${tag}：服务端部署里勾选框不该被禁用 ✗`);
  }
  const newErrors = pageErrors.slice(errStart);
  if (c.localOnly) {
    if (newErrors.length) console.log(`  ⚠️ ${tag}：模拟 localOnly 时页面有 ${newErrors.length} 条异常（服务端页面上不是真 PWA ⇒ 不作判红）`, newErrors.slice(0, 2));
  } else if (newErrors.length) {
    bad(`⑤ ${tag}：页面有 JS 异常 ✗`, newErrors.slice(0, 3));
  }
  console.log(`  ⓘ ${tag}：stored=${JSON.stringify(read.stored)}｜wasmState=${JSON.stringify(read.wasmState)}｜` +
    `stats=${JSON.stringify(read.stats)}｜翻面=${JSON.stringify(flips)}`);
}

// ---------- 收尾：还原偏好 ----------
await send("Page.navigate", { url: `${base}/?v=restore-${Date.now()}` });
await waitFor(`document.readyState === "complete"`, 15000);
await evaluate(`(() => { try {
  const k = ${JSON.stringify(STORAGE_KEY)};
  const v = ${JSON.stringify(originalStored)};
  if (v === null) localStorage.removeItem(k); else localStorage.setItem(k, v);
  return localStorage.getItem(k);
} catch (error) { return "err"; } })()`);
const restored = await evaluate(`(() => { try { return localStorage.getItem(${JSON.stringify(STORAGE_KEY)}); } catch (error) { return "err"; } })()`);
if (restored !== originalStored) {
  bad("收尾没把偏好还原 ⇒ 会污染同 profile 的下一条判据 ✗", { original: originalStored, now: restored });
} else {
  ok("收尾已还原偏好 ✓", { [STORAGE_KEY]: restored });
}

if (pageErrors.length) {
  // 正常五档已在逐档断言里判过红 ✓；这里只把总数打出来 ✓（localOnly 模拟那档不算 ✗）
  console.log(`  ⓘ 页面异常总数（含 localOnly 模拟）：${pageErrors.length}`);
} else {
  ok("页面零 JS 异常 ✓");
}

await sendB("Target.closeTarget", { targetId });
ws.close();
if (problems.length) {
  console.log("  ✗ " + problems.length + " 条不通过：" + problems.join("；"));
  clearTimeout(judgeGuard);
  process.exit(1);
}
console.log("  ✓ 勾选框六档全部在首帧就说真话（启动后的二次对账没翻掉它；本地专用那档节点还在、说明可见）");
clearTimeout(judgeGuard);
process.exit(0);
