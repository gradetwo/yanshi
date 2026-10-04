// **界面双语判据**：默认中文；点顶栏开关切成英文后，**顶栏里不应再有中文**，切回来应恢复中文。
// 为什么这条能红：把开关的处理器去掉、或把词典删空 ⇒ 第 2 步立刻不成立（英文没出现 / 中文仍在）。
// 用法：CDP_PORT=9xxx node scripts/browser-i18n.mjs "<viewer-url>"
const url = process.argv[2];
if (!url) { console.error("用法: node scripts/browser-i18n.mjs <viewer-url>"); process.exit(2); }
const port = process.env.CDP_PORT || "9222";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const socket = new WebSocket(targets.find((x) => x.type === "page").webSocketDebuggerUrl);
let id = 1; const pending = new Map();
socket.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((o) => { socket.onopen = o; });
const send = (method, params) => new Promise((res) => { const i = id++; pending.set(i, res); socket.send(JSON.stringify({ id: i, method, params: params || {} })); });
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable");

const snapshot = `JSON.stringify((() => {
  const header = document.querySelector("header");
  // **排除语言开关自己**：它按惯例显示"切过去会变成哪个语言"（英文界面下就是"中文"二字），
  // 那两个字**本来就该是中文**，不算漏翻。
  const collect = (node, out) => {
    if (!node) return out;
    if (node.nodeType === 3) return out + node.nodeValue;
    if (node.nodeType !== 1) return out;
    if (node.id === "langToggle") return out;
    for (const child of node.childNodes) out = collect(child, out);
    return out;
  };
  const chinese = collect(header, "").match(/[\\u4e00-\\u9fff]/g) || [];
  const button = document.getElementById("langToggle");
  return {
    htmlLang: document.documentElement.lang,
    newDoc: (document.getElementById("newDoc") || {}).textContent,
    toolbar: (document.getElementById("toggleRail") || {}).textContent,
    toggle: button ? button.textContent : null,
    headerChinese: chinese.length,
    headerChineseText: chinese.slice(0, 8).join(""),
  };
})())`;

const failures = [];
await send("Page.navigate", { url });
await sleep(6000);
const zh = JSON.parse(await evaluate(snapshot));
console.log("  【默认】" + JSON.stringify(zh));
if (zh.htmlLang !== "zh-CN") failures.push(`默认 html lang 应为 zh-CN，实为 ${zh.htmlLang}`);
if (!String(zh.newDoc).includes("新建")) failures.push(`默认「新建」按钮应为中文，实为 ${zh.newDoc}`);
if (zh.headerChinese === 0) failures.push("默认顶栏里没有中文 ⇒ 前置条件不成立");

// 点开关 ⇒ 页面带 ?lang=en 重载 ⇒ 等到英文生效
await evaluate(`(() => { document.getElementById("langToggle").click(); return "clicked"; })()`);
let en = null;
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { en = JSON.parse(await evaluate(snapshot)); } catch (_) { continue; }
  if (en && en.htmlLang === "en") break;
}
console.log("  【英文】" + JSON.stringify(en));
if (!en || en.htmlLang !== "en") failures.push("切到英文后 html lang 不是 en");
else {
  if (String(en.newDoc).trim() !== "New") failures.push(`英文下「新建」应为 New，实为 ${en.newDoc}`);
  if (!String(en.toolbar).includes("Toolbar")) failures.push(`英文下工具栏开关应含 Toolbar，实为 ${en.toolbar}`);
  if (String(en.toggle).trim() !== "中文") failures.push(`英文下开关应显示「中文」，实为 ${en.toggle}`);
  // **最强的一条**：顶栏里**一个中文都不该剩**（漏翻立刻红）
  if (en.headerChinese > 0) failures.push(`英文顶栏里仍有中文 ${en.headerChinese} 个：${en.headerChineseText}`);
}

// 再点一次 ⇒ 回到中文
await evaluate(`(() => { document.getElementById("langToggle").click(); return "clicked"; })()`);
let back = null;
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { back = JSON.parse(await evaluate(snapshot)); } catch (_) { continue; }
  if (back && back.htmlLang === "zh-CN" && String(back.newDoc).includes("新建")) break;
}
console.log("  【切回】" + JSON.stringify(back));
if (!back || back.htmlLang !== "zh-CN") failures.push("切回中文后 html lang 不是 zh-CN");
else if (!String(back.newDoc).includes("新建")) failures.push(`切回后「新建」应为中文，实为 ${back.newDoc}`);

socket.close();
if (failures.length) { console.log("  ✗ 双语切换不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ 双语切换：默认中文、切英文后顶栏无中文、切回中文");
process.exit(0);
