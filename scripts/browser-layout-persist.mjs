#!/usr/bin/env node
// criterion-target: client ## 运行器据此注入 render=client（测本地内核）
// **★ 总超时守卫 ✗ ★**：浏览器/CDP 起不来时报「自己跑不了」并退 3（与产品失败区分）；`unref()` ⇒ 跑完不被它拖住。
const __judgeTimeout = setTimeout(() => {
console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 300000));
__judgeTimeout.unref();

// **布局持久化判据**：右栏标签页与画笔区折叠存 `localStorage` 的 `yanshi.layout` ✓
// ⇒ 改布局 → 重载 → 恢复一致；顶栏「恢复默认布局」→ 立即回默认且清掉已存值 → 重载仍默认 ✓。
//
// **语言前提显式钉死**（i18n 线的实测教训 ✓）：存量判据不带 lang 前提时，
// 断言中文界面的判据会在非中文浏览器下被自动检测翻面 ⇒ 本判据先把
// `navigator.language` 钉到 zh-CN（文档注入，与 browser-i18n 同法）
// ＋ 在壳页把 `yanshi.lang` 写成 zh、擦掉四个布局键（无存储起点）再进目标页 ✓。
// 钉不住 ⇒ 前提不成立 ⇒ 退出码 3（不是产品失败）。
//
// **断言读结构量不断像素**：当前标签页（`yanshiRightTabs.state().active`）、
// 画笔区折叠（`body.brush-area-collapsed`）、存储原文 —— 都与视口尺寸无关 ✓。
// **改布局走真实用户路径**：点标签页按钮、点画笔区开关、点「恢复默认布局」✓ 不直接调内部函数 ✗。
//
// 用法：node scripts/browser-layout-persist.mjs "<viewer-url>"
const url = process.argv[2];
if (!url) { console.error("用法: node scripts/browser-layout-persist.mjs <viewer-url>"); process.exit(2);}
const rawPort = process.env.CDP_PORT || process.argv[3] || "";
const port = /^\d+$/.test(rawPort)? rawPort: "9222";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let targets;
try {
targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
process.exit(3);
}
const target = targets.find((x) => x.type === "page");
if (!target) { console.error("❌ 判据自身的求值失败（不是产品问题）⇒ CDP 目标里没有 page ⇒ 退出码 3"); process.exit(3);}
const socket = new WebSocket(target.webSocketDebuggerUrl);
let id = 1; const pending = new Map();
const consoleErrors = [];
socket.onmessage = (e) => {
const m = JSON.parse(e.data);
if (m.method === "Runtime.exceptionThrown") {
const d = m.params.exceptionDetails || {};
consoleErrors.push("exception: " + (d.exception?.description || d.text || ""));
}
if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") {
consoleErrors.push("console: " + (m.params.args || []).map((a) => a.value?? a.description?? "").join(" "));
}
if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id);}
};
await new Promise((o) => { socket.onopen = o;});
const send = (method, params) => new Promise((res) => { const i = id++; pending.set(i, res); socket.send(JSON.stringify({ id: i, method, params: params || {}}));});
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true})).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable");

const failures = [];
const check = (ok, label, detail) => {
console.log(` ${ok? "✓": "✗"} ${label}${detail!== undefined? `（${detail}）`: ""}`);
if (!ok) failures.push(label + (detail!== undefined? `：${detail}`: ""));
};

// ---- 前提 1：浏览器语言钉到 zh-CN（文档注入；验钉，钉不住退 3） ----
const pin = await send("Page.addScriptToEvaluateOnNewDocument", {
source: `Object.defineProperty(Navigator.prototype, "language", { configurable: true, get: () => "zh-CN"});`,
});
if (!pin?.result?.identifier) {
console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 语言注入没返回脚本标识 ⇒ 退出码 3");
process.exit(3);
}
const shellUrl = url.split("?")[0].replace(/\/+$/, "") + "/?v=" + Date.now();
await send("Page.navigate", { url: shellUrl});
await sleep(700);
const navLang = await evaluate(`navigator.language`);
if (!String(navLang).toLowerCase().startsWith("zh")) {
console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 浏览器语言没被钉到 zh（实为 " + JSON.stringify(navLang) + "）⇒ 退出码 3");
process.exit(3);
}
// ---- 前提 2：壳页上写死语言选择、擦掉全部布局键（无存储起点） ----
const wiped = await evaluate(`(() => {
try {
localStorage.setItem("yanshi.lang", "zh");
for (const k of ["yanshi.panels", "yanshi.dockers", "yanshi.workspace", "yanshi.layout"]) localStorage.removeItem(k);
return localStorage.getItem("yanshi.layout") === null? "clean": "STILL-SET";
} catch (_) { return "NO-STORAGE";}
})()`);
if (wiped!== "clean") {
console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 擦不掉布局键（实为 " + JSON.stringify(wiped) + "）⇒ 退出码 3");
process.exit(3);
}

// ---- 就绪与快照（等待条件与断言条件分开：只等钩子齐备，不等某个布局值） ----
const waitReady = async () => {
for (let i = 0; i < 40; i++) {
await sleep(300);
try {
if (await evaluate(`document.readyState === "complete" &&!!window.yanshiRightTabs &&!!window.yanshiBrushArea &&!!document.getElementById("resetLayout")`)) return true;
} catch (_) { /* 还没就绪 ⇒ 继续等 */}
}
return false;
};
const snapshot = async () => JSON.parse(await evaluate(`JSON.stringify((() => {
let stored = null;
try { stored = localStorage.getItem("yanshi.layout");} catch (_) {}
return {
tab: window.yanshiRightTabs? window.yanshiRightTabs.state().active: null,
brush: document.body.classList.contains("brush-area-collapsed"),
stored,
};
})())`));

// ---- 阶段 1：无存储 ⇒ 默认布局（标签页 paint、画笔区展开） ----
await send("Page.navigate", { url});
check(await waitReady(), "页面就绪（标签页/画笔区/恢复默认按钮钩子齐备）");
let snap = await snapshot();
console.log(" " + JSON.stringify(snap));
check(snap.tab === "paint", "无存储时右栏停在「绘制」标签页", `实为 ${snap.tab}`);
check(snap.brush === false, "无存储时画笔区展开", `实为 ${snap.brush}`);

// ---- 阶段 2：真实点击改布局 ⇒ 状态变 + 落盘 ----
const picked = await evaluate(`(() => {
const btn = [...document.querySelectorAll('#rightTabs button[data-tab]')].find((b) =>!b.hidden && b.dataset.tab!== "paint");
if (!btn) return null;
btn.click();
return btn.dataset.tab;
})()`);
check(typeof picked === "string" && picked.length > 0, "存在可切的非默认标签页并已点", `实为 ${JSON.stringify(picked)}`);
await evaluate(`(() => { document.getElementById("brushAreaToggle").click(); return "clicked";})()`);
await sleep(200);
snap = await snapshot();
console.log(" " + JSON.stringify(snap));
check(snap.tab === picked, "点后标签页已切换", `期望 ${picked}，实为 ${snap.tab}`);
check(snap.brush === true, "点后画笔区已折叠", `实为 ${snap.brush}`);
let storedObj = null;
try { storedObj = JSON.parse(snap.stored || "null");} catch (_) { storedObj = null;}
check(!!storedObj && storedObj.v === 1 && storedObj.tab === picked && storedObj.brush === true,
"布局已落盘到 yanshi.layout（v/tab/brush 与界面一致）", `实为 ${snap.stored}`);

// ---- 阶段 3：重载 ⇒ 恢复一致 ----
await send("Page.navigate", { url});
check(await waitReady(), "重载后页面就绪");
snap = await snapshot();
console.log(" " + JSON.stringify(snap));
check(snap.tab === picked, "重载后标签页恢复", `期望 ${picked}，实为 ${snap.tab}`);
check(snap.brush === true, "重载后画笔区仍折叠", `实为 ${snap.brush}`);

// ---- 阶段 4：点「恢复默认布局」⇒ 立即回默认 + 已存值清掉 ----
await evaluate(`(() => { document.getElementById("resetLayout").click(); return "clicked";})()`);
await sleep(200);
snap = await snapshot();
console.log(" " + JSON.stringify(snap));
check(snap.tab === "paint", "恢复默认后标签页回「绘制」", `实为 ${snap.tab}`);
check(snap.brush === false, "恢复默认后画笔区展开", `实为 ${snap.brush}`);
check(snap.stored === null, "恢复默认后 yanshi.layout 已清掉", `实为 ${snap.stored}`);

// ---- 阶段 5：再重载 ⇒ 仍是默认（重置不是只改内存） ----
await send("Page.navigate", { url});
check(await waitReady(), "重置后重载页面就绪");
snap = await snapshot();
console.log(" " + JSON.stringify(snap));
check(snap.tab === "paint" && snap.brush === false, "重置后重载仍是默认布局", JSON.stringify(snap));

// ---- 阶段 6：损坏/旧版数据 ⇒ 安全降级（不白屏、未知 tab 落回默认、合法字段仍生效） ----
await evaluate(`(() => { try { localStorage.setItem("yanshi.layout", "{bad json");} catch (_) {} return "set";})()`);
await send("Page.navigate", { url});
check(await waitReady(), "存量是坏 JSON 时页面仍就绪（不白屏）");
snap = await snapshot();
check(snap.tab === "paint" && snap.brush === false, "坏 JSON 整份丢弃回默认", JSON.stringify(snap));
await evaluate(`(() => { try { localStorage.setItem("yanshi.layout", JSON.stringify({ v: 1, tab: "nope", brush: true}));} catch (_) {} return "set";})()`);
await send("Page.navigate", { url});
check(await waitReady(), "存量含未知 tab 时页面仍就绪");
snap = await snapshot();
console.log(" " + JSON.stringify(snap));
check(snap.tab === "paint", "未知 tab 落回默认「绘制」", `实为 ${snap.tab}`);
check(snap.brush === true, "同份存量里的合法字段（brush）仍生效", `实为 ${snap.brush}`);
await evaluate(`(() => { try { localStorage.removeItem("yanshi.layout");} catch (_) {} return "cleaned";})()`);

check(consoleErrors.length === 0, "全程零控制台错误/异常", consoleErrors.slice(0, 3).join(" | ") || undefined);

socket.close();
if (failures.length) {
console.log(" ✗ 布局持久化不合格：");
for (const f of failures) console.log(" - " + f);
process.exit(1);
}
console.log(" ✓ 布局持久化：改布局落盘、重载恢复、恢复默认清存值且重载仍默认、损坏数据安全降级");
process.exit(0);
