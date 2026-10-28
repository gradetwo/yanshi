// criterion-target: static  ## 运行器据此决定是否注入 render=client（测静态文案（本地模式会把 endpoint_not_local 噪声算进中文））
// **界面双语判据**：默认中文；点顶栏开关切成英文后，**顶栏里不应再有中文**，切回来应恢复中文。

// **★ 总超时守卫 ✗ ★**（2026-10-16 ✓）：**本判据要连 CDP 浏览器** ✓ ⇒ 浏览器起不来时它可能**一直等**
//   （实测：CI 的判据分片被这种判据**拖到跑不完** ✗ ⇒ 我这边十几轮拿不到判定 ✓）。
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✗，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
//   ⇒ **∴ 超时就报「无法判断」并退 3** ✓ —— **不冒充产品失败** ✗、**也不永远挂着** ✗ ★
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 90000);
// 为什么这条能红：把开关的处理器去掉、或把词典删空 ⇒ 第 2 步立刻不成立（英文没出现 / 中文仍在）。
// 用法：CDP_PORT=9xxx node scripts/browser-i18n.mjs "<viewer-url>"
const url = process.argv[2];
if (!url) { console.error("用法: node scripts/browser-i18n.mjs <viewer-url>"); process.exit(2); }
const port = process.env.CDP_PORT || "9222";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// **★ CDP 连接守卫 ✗ ★**（2026-10-16 ✓）：浏览器没起来时这里会抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
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
  // **第 931 轮：把 #last 也排除** ✓ —— 它**不是 UI 标签** ✓，而是**「最近一次响应」的输出区** ✓
  //（viewer.rs:629 的标题是「最近一次响应」✓）⇒ 页面里的 API JSON 会被算成「中文」✗ ⇒ 判据**间歇红** ✗。
  // 观测证据：where 报 div#last ✓、sample 是 API 的 JSON（count/ok/persisted 那些字段）✓。
  const skip = (el) => el.closest("#log, #last, pre, code, script, style, textarea, #langToggle, [data-i18n='off']");
  // **★ 「这串是数据」由产品自己声明 ✓ 判据不猜 ✗ ★**（第六轮实测 ✓）：
  //   减淡加深卡片会**把用户的图层名念进句子里** ✓（「将新建组 「图层 1 · 减淡加深」」✓）
  //   ⇒ 英文模式下界面把它翻成 Will build the group "图层 1 · Dodge and burn" ✓
  //   **框架全英文 ✓ 剩下的是用户起的名字** ✓ ⇒ 那不是漏翻 ✗ 判据原来却把它算成"整页还有中文" ✓
  //   （同一条道理 browser-tooltip-i18n 早就用了 ✓：属性里带数据 ⇒ 只数框架 ✓）。
  //   **⇒ 豁免的边界**：只认**产品自己标了 data-i18n="off" 的那些串** ✓（图层名、字体名 ✓
  //     见 viewer-app.js 的 data-i18n="off" ✓ 与 viewer.rs 的字体 option ✓）
  //     ⇒ 想蒙过这条判据 ✗ 得先在产品里公开把那段文字声明成数据 ✓ 那是可评审的动作 ✗ 不是判据自己放宽 ✗。
  //   **反证**：若界面哪天真的漏了「减淡加深」没翻 ⇒ 剥掉图层名之后**还剩汉字** ⇒ 照样红 ✓。
  const dataStrings = (() => {
    const out = [];
    for (const el of document.querySelectorAll("[data-i18n='off']")) {
      if (el.closest("#log, pre, code, script, style, textarea")) continue;
      const t = String(el.textContent || "").trim();
      if (t && zh.test(t)) out.push(t);
    }
    // 长的先剥 ✓ 免得短的先剥掉把长的切成碎片 ✗
    out.sort((a, b) => b.length - a.length);
    return out;
  })();
  const found = new Map();
  const dataExempt = [];
  const where = [];
  const walk = (node) => {
    if (node.nodeType === 3) {
      const text = node.nodeValue.trim();
      if (text && zh.test(text) && !skip(node.parentElement)) {
        let residue = text;
        let hitData = null;
        for (const d of dataStrings) {
          if (residue.indexOf(d) >= 0) { hitData = d; residue = residue.split(d).join(""); }
        }
        if (!zh.test(residue)) {
          if (hitData) dataExempt.push(text.slice(0, 60));
        } else {
          const key = text.slice(0, 60);
          found.set(key, (found.get(key) || 0) + 1);
        // **第 926 轮：加观测看它到底在哪个元素里** ✓（**读数停滞时加观测** ✗，而不是继续猜 ✓）：
        // 「页面里还有 1 条中文」✗ 而那条是 **API 的 JSON 响应** ✓ ⇒ **它在哪个容器 ⇒ 决定 skip 该怎么补** ✓。
        const el = node.parentElement;
        // ⚠️ **这里不能再用反引号/嵌套模板** ✗（第 926 轮踩了两次 ✓）：bodyProbe 本身就是模板字符串 ✓。
        if (el) where.push("<" + el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") + (el.className ? "." + String(el.className).split(" ")[0] : "") + ">");
        }
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
    dataMarks: dataStrings.length, dataExempt: dataExempt.slice(0, 3),

    paletteVisible: !!(cardPalette && cardPalette.offsetParent),
    // **为什么不可见** ✗ —— 只报布尔值无法定位（CI 实测 paletteVisible:false ✓，
    // 而 cardPalette **没有 hidden 属性** ✓、仓库里也**没有**会藏 data-panel=assets 的
    // @media 块 ✓）⇒ 把**祖先链**的 display／visibility／overflow 一起打出来 ✓。
    // ⚠️ 这段在**模板字面量**里 ⇒ 注释里**绝不能出现反引号**✗（会终止模板 ⇒ SyntaxError ✓）。
    paletteChain: (() => {
      const out = [];
      let el = cardPalette;
      for (let i = 0; el && i < 12; i += 1, el = el.parentElement) {
        const cs = getComputedStyle(el);
        out.push((el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") +
          (el.className ? "." + String(el.className).split(" ")[0] : "")) +
          "[" + cs.display + "/" + cs.visibility + (cs.overflow !== "visible" ? "/" + cs.overflow : "") + "]");
      }
      return out;
    })(),
    labels: { refresh: body.includes("Refresh"), palette: body.includes("Palette"),
      file: body.includes("File") } };
})())`;

const failures = [];
// **★ 先擦语言偏好，再断言「默认是中文」✗ ★**（第六轮实测 ✓）：本判据第一条断言是**默认中文** ✓
//   而 `viewer-app.js:13573-13811` 把选择记在 `localStorage.yanshi.lang`（**优先级高于默认值** ✓）
//   ⇒ 同一个 CDP profile 里**上一条判据**切过英文 ⇒ 这里的「默认」就被污染 ✗
//   （实测：连跑 `browser-layer-lock`（⑧ 要切英文）之后再跑本判据 ⇒【默认】读到 `htmlLang:"en"` ✓
//    三条断言整体错位一位 ⇒ 看着像双语功能坏了 ✗ 其实功能是好的 ✓）。
//   CI 每个分片**新 profile** ⇒ 看不出来 ⇒ 这正是「本机红 / CI 绿」那一类假红 ✓。
//   ⇒ 口径与 `browser-fill-region` / `browser-layer-lock` 一致 ✓：**先落到壳页擦偏好 ✓ 再进目标 URL** ✓。
//   ⚠️ **擦的是前置条件 ✗ 不是被判对象**：这里抹掉的是"用户存过的偏好" ✓
//      而「没有偏好时**必须**默认中文」仍由下面的断言判 ✗（第 887 轮那条教训 ✓）。
const shellUrl = url.split("?")[0].replace(/\/+$/, "") + "/?v=" + Date.now();
await send("Page.navigate", { url: shellUrl });
await sleep(700);
const wiped = await evaluate(`(() => { try { localStorage.removeItem("yanshi.lang"); return localStorage.getItem("yanshi.lang") === null ? "clean" : "STILL-SET"; } catch (_) { return "NO-STORAGE"; } })()`);
if (wiped !== "clean") {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 擦不掉 `yanshi.lang`（实为 " + JSON.stringify(wiped) + "）⇒ 「默认中文」这条断言的前提不成立 ⇒ 退出码 3");
  process.exit(3);
}
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
  // **豁免必须说出来 ✗ 不能悄悄少报** ✓：数到了几条"产品自己声明是数据"的串 ✓ 有几条文本因此不算漏翻 ✓
  //（与 `browser-tooltip-i18n` 同一口径 ✓：那条会打「其中 N 条含产品自己标过的用户数据」✓）。
  if (probe.dataExempt && probe.dataExempt.length > 0) {
    console.log("  【数据豁免】产品标了 " + probe.dataMarks + " 条 `data-i18n=off` 的数据串 ✓ " +
      "因此不算漏翻的文本 " + JSON.stringify(probe.dataExempt));
  }
// **调色板为何不可见** ✗（CI 实测 `paletteVisible:false` ✓）：打印**祖先链**的 `display`／`visibility` ✓
// ⇒ 一眼看出是谁藏了它（**∴ 若整条链都正常显示 ⇒ 那就是 `offsetParent` 这类判据侧陷阱 ✓**）。
if (!probe.paletteVisible) console.log("  【调色板祖先链】" + JSON.stringify(probe.paletteChain));
  if (probe.distinct > 0) failures.push(`英文模式下整页仍有 ${probe.distinct} 条中文：${JSON.stringify(probe.sample)}`);
  // 只断言"默认布局下一定可见"的标签（工具栏刷新 ✓、调色板面板 ✓、顶栏文件菜单 ✓）；
  // 历史/标注等面板默认收起 ⇒ 断言它们存在等于假设布局，会很脆。
  // **只断言"此刻真的可见"的面板** ✗ —— `paletteChain` 实测（2026-10-06 CI ✓）：
  // `cardPalette` **自己**是 `block/visible` ✓，但它的父层 `div.tab-pane` 是 **`display:none`** ✗
  // ⇒ 那个 tab 当前**没被选中** ⇒ `offsetParent` 为 `null` ⇒ 里面的英文标签**取不到** ✓。
  // ⇒ 这不是"翻译缺失" ✗（**∴ `【整页】distinct:0` ✓**），而是**断言了一个不可见的面板** ✗
  // ⇒ 与上面那句注释里"历史/标注等面板默认收起 ⇒ 断言它们存在等于假设布局，会很脆"**同一条道理** ✓。
  const labelVisible = { refresh: true, file: true, palette: probe.paletteVisible };
  for (const [name, ok] of Object.entries(probe.labels)) {
    if (labelVisible[name] === false) continue;      // 面板此刻不可见 ⇒ 不断言它的标签 ✓
    if (!ok) failures.push(`英文模式下看不到英文标签 ${name}`);
  }
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
