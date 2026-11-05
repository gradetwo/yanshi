#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
//
// **★ 总超时守卫 ✗ ★**：浏览器/CDP 起不来时本判据会一直等 ⇒ 超时**报「自己跑不了」并退 3**。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 90000));
__judgeTimeout.unref();

// **「打开文档」对话框里的示例卡片不许被压扁** ✓（2026-11-05 真机截图实证 ✓）
//
// **∴ 当时的症状** ✗：全局 `button { height: var(--control-h) }`（28px）把六张示例卡片
//   全压成 28px ✓ 而每张卡里是「标题 18 ＋ 说明 33」 ✓ ⇒ `overflow: visible`
//   ⇒ **说明文字压到下一行卡片上**⑦（截图：四张卡片的说明与第二排标题叠在一起 ✓）。
//
// **∴ 判什么（可量、可红 ✓）**：
//   ① 六张卡都在 ✓（少一张说明列表没渲染完 ⇒ 不可判，退 3）；
//   ② 每张卡**装得下自己的内容**：`scrollHeight <= clientHeight + 1` ✓
//      （压扁时 51 > 28 ⇒ 直接红 ✓）；
//   ③ 每张卡**高过一行文字**（`height >= 40`）✓ —— 上一条的等价形式，但读数更直白 ✓。
//
// **∴ 变异**：把 `height:auto` 从卡片样式里去掉 ⇒ ② 与 ③ 同时红 ✓。
//
// 用法：node scripts/browser-pwa-samples.mjs <viewer-url>
const url = process.env.YANSHI_VIEWER_URL || process.argv[2];
if (!url) { console.error("用法: node scripts/browser-pwa-samples.mjs <viewer-url>"); process.exit(2); }
const port = process.env.CDP_PORT || "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）"); process.exit(3); }

const ws = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
ws.onmessage = (event) => {
  const m = JSON.parse(event.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
await new Promise((open) => { ws.onopen = open; });
const send = (method, params) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  ws.send(JSON.stringify({ id, method, params: params || {} }));
});
const evaluate = async (expression) => {
  const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const details = reply?.result?.exceptionDetails;
  if (details) {
    const text = details.exception?.description || details.exception?.value || details.text || "（无描述）";
    throw new Error("页面侧抛错：" + String(text).split("\n").slice(0, 3).join(" ｜ "));
  }
  return reply?.result?.result?.value;
};
await send("Runtime.enable");
await send("Page.enable");

const docId = "pwa-samples-" + Date.now().toString(36);
const sep = url.includes("?") ? "&" : "?";
await send("Page.navigate", { url: `${url}${sep}doc=${docId}&token=local&render=client` });
for (let i = 0; i < 80; i += 1) {
  await sleep(250);
  try { if (await evaluate('document.readyState === "complete" && !!document.querySelector("#tools button")')) break; } catch (_) { /* 还没就绪 */ }
}

// **打开「文件 → 打开」** ✓（与创作者点的完全同一条路 ✓）。
const opened = await evaluate(`(async () => {
  const fileButton = [...document.querySelectorAll("header button")].find((b) => (b.textContent || "").includes("文件"));
  if (!fileButton) return "no-file-button";
  fileButton.click();
  await new Promise((r) => setTimeout(r, 250));
  const openItem = [...document.querySelectorAll("#fileMenu button, #fileMenu a")].find((b) => (b.textContent || "").includes("打开"));
  if (!openItem) return "no-open-item";
  openItem.click();
  await new Promise((r) => setTimeout(r, 800));
  return "opened";
})()`);
if (opened !== "opened") {
  console.log("  ⊘ 不可判：打不开「文件 → 打开」（" + JSON.stringify(opened) + "）⇒ 不是产品结论");
  ws.close();
  process.exit(3);
}

// 列表是同步渲染的 ✓；仍**有界**等一下（慢机器上 dialog 的布局可能还没算完 ✓）。
let measured = null;
for (let i = 0; i < 24; i += 1) {
  measured = await evaluate(`(() => {
    const cards = [...document.querySelectorAll("#sampleList > button")];
    return cards.map((card) => ({
      text: (card.textContent || "").trim().slice(0, 14),
      height: Math.round(card.getBoundingClientRect().height),
      client: card.clientHeight,
      scroll: card.scrollHeight,
    }));
  })()`);
  if (Array.isArray(measured) && measured.length >= 6) break;
  await sleep(250);
}
console.log("  示例卡片：" + JSON.stringify(measured));
if (!Array.isArray(measured) || measured.length < 6) {
  console.log("  ⊘ 不可判：示例卡片只渲染出 " + (Array.isArray(measured) ? measured.length : "?") + " 张（应为 6）⇒ 本次没验");
  ws.close();
  process.exit(3);
}

const failures = [];
for (const card of measured) {
  if (card.scroll > card.client + 1) {
    failures.push(`卡片「${card.text}」装不下内容（scroll ${card.scroll} > client ${card.client}）⇒ 文字会压到下一行 ✗`);
  }
  if (card.height < 40) {
    failures.push(`卡片「${card.text}」只有 ${card.height}px（不足两行）⇒ 被全局控件高度压扁了 ✗`);
  }
}

if (failures.length) {
  console.log("  ✗ " + failures.join("；"));
  ws.close();
  process.exit(1);
}
console.log("  ✓ 六张示例卡片都按内容长高（无溢出、无压行）");
ws.close();
process.exit(0);
