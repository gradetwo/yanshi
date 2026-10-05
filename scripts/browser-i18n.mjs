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

const bodyProbe = `JSON.stringify((() => {
  const zh = /[\\u4e00-\\u9fff]/;
  const skip = (el) => el.closest("#log, pre, code, script, style, textarea, #langToggle, [data-i18n='off']");
  const found = new Map();
  const where = [];
  const walk = (node) => {
    if (node.nodeType === 3) {
      const text = node.nodeValue.trim();
      if (text && zh.test(text) && !skip(node.parentElement)) {
        const key = text.slice(0, 60);
        found.set(key, (found.get(key) || 0) + 1);
        // **第 926 轮：加观测看它到底在哪个元素里** ✓（**读数停滞时加观测** ✗，而不是继续猜 ✓）：
        // 「页面里还有 1 条中文」✗ 而那条是 **API 的 JSON 响应** ✓ ⇒ **它在哪个容器 ⇒ 决定 skip 该怎么补** ✓。
        const el = node.parentElement;
        // ⚠️ **这里不能再用反引号/嵌套模板** ✗（第 926 轮踩了两次 ✓）：bodyProbe 本身就是模板字符串 ✓。
        if (el) where.push("<" + el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") + (el.className ? "." + String(el.className).split(" ")[0] : "") + ">");
      }
      return;
    }
    if (node.nodeType !== 1) return;
    for (const child of node.childNodes) walk(child);
  };
  walk(document.body);
  const body = document.body.innerText;
  // 第 769 轮：labels 走 innerText（只含可见），而 distinct 走全节点遍历 ⇒ 口径不同。
  // 所以这里直接打印那个面板此刻是否可见，一次就能判断 palette:false 的含义。
  const cardPalette = document.getElementById("cardPalette");
  return { distinct: found.size, where: where.slice(0, 6), sample: [...found.keys()].slice(0, 6),
    paletteVisible: !!(cardPalette && cardPalette.offsetParent),
    labels: { refresh: body.includes("Refresh"), palette: body.includes("Palette"),
      file: body.includes("File") } };
})())`;

const failures = [];
await send("Page.navigate", { url });
// **等到"页面真的就绪"，而不是固定睡 6 秒** ✗（第 886 轮 ✓）：
// 本文件在**切换语言后**用的是"轮询 40×300ms 直到条件成立" ✓（见下面两处 ✓），
// 却在**首次导航后**用了固定 `sleep(6000)` ✗ ⇒ 机器慢一点 ⇒ 快照就是"没加载完"的状态 ✗
// ⇒ ⇒ **这条判据的间歇红就出在这里** ✓（第 800 轮实测：同一提交有时绿有时红 ✓）。
// ⚠️ **等待条件必须与断言条件分开** ✗（第 887 轮 ✓，我第一版写错过 ✓）：
// 我第一版"等到 `htmlLang === "zh-CN"` 才 break" ✗ ⇒ **那等于把被判条件当等待条件** ✗ ⇒
// 「默认语言必须是中文」这条断言就**几乎永远不会失败** ✗（**判据被自己削弱了** ✓）。
// ⇒ 现在等到的是**独立的就绪信号** ✓：`readyState === "complete"` 且语言开关已存在 ✓；
//   **"是不是中文"仍然只由下面的断言判** ✓ ✓。
let ready = false;
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try {
    if (await evaluate(`document.readyState === "complete" && !!document.getElementById("langToggle")`)) { ready = true; break; }
  } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
const zh = JSON.parse(await evaluate(snapshot));
if (!ready) failures.push("等待页面就绪超时（readyState 未到 complete 或缺 langToggle）⇒ 下面的断言可能不可信");
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
  // **更强的一条**：英文模式下**整页**（除日志、代码示例、语言开关）**不应再有中文**。
  const probe = JSON.parse(await evaluate(bodyProbe));
  console.log("  【整页】" + JSON.stringify(probe));
  if (probe.distinct > 0) failures.push(`英文模式下整页仍有 ${probe.distinct} 条中文：${JSON.stringify(probe.sample)}`);
  // 只断言"默认布局下一定可见"的标签（工具栏刷新 ✓、调色板面板 ✓、顶栏文件菜单 ✓）；
  // 历史/标注等面板默认收起 ⇒ 断言它们存在等于假设布局，会很脆。
  for (const [name, ok] of Object.entries(probe.labels)) if (!ok) failures.push(`英文模式下看不到英文标签 ${name}`);
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
