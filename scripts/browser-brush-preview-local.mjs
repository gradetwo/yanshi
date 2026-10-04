#!/usr/bin/env node
// ⚠ 模板字符串里绝对不要写反引号（会提前结束模板 ⇒ SyntaxError ✓，本仓库已吃过三次）。
//
// **画笔库的预览是"本地先出图、且如实标注近似"** —— 第 47 轮那条改动的判据 ✓。
//
// **判据（都能红）**：
//   ① 画笔库里**有行**（≥1）✓ —— 第 47 轮的探针报 0 行 ✗，是因为没先点画笔工具/等列表载入 ✓；
//   ⚠️ **已登记为已知红** ✓（第 434 轮 ✓）：②③④ 期待的接口**产品里不存在** ✗ ——
//   全仓搜 `preview-source` / `previewSource` 只命中**本文件两处** ✓（`viewer.rs` 里连这个字符串都没有 ✗）；
//   而 `labelled` 期待的 `title` 含「近似」**同样零命中** ✗（`grep 近似 viewer.rs` ⇒ 空 ✓）。
//   ⇒ **本判据测的是一个尚未实现的接口** ✓（「笔刷列表的预览由本地门面出图」这件事产品没做 ✓）
//   ⇒ **不是产品缺陷** ✗；要修的是**产品是否提供该接口**（或本判据改测别的真实信号 ✓）。
//   ② 至少一行带 `data-preview-source="local"` ✓（= 图是本地门面画的 ✓）；
//   ③ 本地那行的 `img.title` **必须写明是近似** ✗（不许假装与服务端逐字节相同 ✓）；
//   ④ 本地图的 `src` 必须是 `data:image/png` ✓（= 没等服务端往返 ✓）。
const url = process.argv[2];
const port = process.env.CDP_PORT || "9345";
if (!url) { console.error("用法: CDP_PORT=.. node scripts/browser-brush-preview-local.mjs <viewer-url>"); process.exit(2); }
const list = await fetch(`http://127.0.0.1:${port}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page");
if (!target) { console.error("未找到调试目标"); process.exit(2); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1; const pending = new Map();
ws.addEventListener("message", (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) => new Promise((resolve) => { const i = id++; pending.set(i, resolve); ws.send(JSON.stringify({ id: i, method, params })); });
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await send("Runtime.enable"); await send("Page.enable");
await send("Page.navigate", { url }); await send("Page.reload", { ignoreCache: true }); await sleep(1800);
// **先等文档就绪**（别的探针都有这一步；上一版漏了它 ⇒ 笔刷下拉只有 1 个占位项、一行都渲染不出来）。
const readyUntil = Date.now() + 20000;
while (Date.now() < readyUntil) {
  if (await evaluate('document.getElementById("board").width > 400 && window.yanshi.state().serverBlits > 0')) break;
  await sleep(200);
}
// **再点画笔工具**（否则笔刷下拉不会载入 ✓ —— 这正是上次 0 行的原因 ✗）。
await evaluate('document.querySelector("button[data-tool=brush]")?.click()');
const until = Date.now() + 20000;
while (Date.now() < until) {
  if ((await evaluate('document.querySelectorAll("#brush option").length')) > 5) break;
  await sleep(200);
}
await evaluate('document.getElementById("brushLibraryOpen").click()');
await sleep(600);
await evaluate('(() => { const box = document.getElementById("brushLibrary"); box.scrollTop = 0; })()');
await sleep(2500);
const info = await evaluate(`(() => {
  const rows = [...document.querySelectorAll(".brush-lib-row")];
  const local = rows.filter((row) => row.dataset.previewSource === "local");
  const labelled = local.filter((row) => ((row.querySelector("img") || {}).title || "").includes("近似"));
  const dataUrls = local.filter((row) => ((row.querySelector("img") || {}).src || "").startsWith("data:image/png"));
  const sample = local[0] || rows[0];
  return {
    options: document.querySelectorAll("#brush option").length,
    rows: rows.length, local: local.length, labelled: labelled.length, dataUrls: dataUrls.length,
    firstTitle: sample ? ((sample.querySelector("img") || {}).title || "").slice(0, 30) : null,
  };
})()`);
console.log("  " + JSON.stringify(info));
let ok = true;
if (!info || info.rows < 1) { console.error("❌ 画笔库里没有行 ✗（交互顺序或面板未开）"); ok = false; }
else if (info.local < 1) { console.error("❌ 没有任何一行是本地出图的 ✗"); ok = false; }
else if (info.labelled !== info.local) { console.error("❌ 本地预览没有全部标注为近似 ✗"); ok = false; }
else if (info.dataUrls !== info.local) { console.error("❌ 本地预览不是 data: 图（仍在等服务端？）✗"); ok = false; }
if (ok) console.log(`  ✅ 本地预览就位：${info.local} 行本地出图、全部标注为近似、且没有等服务端 ✓`);
ws.close();
process.exit(ok ? 0 : 1);
