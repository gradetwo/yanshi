#!/usr/bin/env node
// **★ 给浏览器判据**预置「客户端渲染**偏好**」✗ ★**（第 359 轮 ✓）
//
// **∴ 为什么必须 ✗**：**产品的默认值是**服务端渲染**✗
//   （**`crates/yanshi-http/assets/viewer-app.js:51`**：
//     `return stored === null ? true : stored === "1";` ✓）
//     ⇒ **∴ 而** `initWasm()`（**第 1513 行 ✓）**第一句就是
//       `if (serverRenderPreferred()) { …; return; }`**✗
//         ⇒ **∴ 于是**：**无偏好时**内核**根本不加载** ✓
//           ⇒ **∴ 而**所有**需要内核**的判据**必然红** ✓（**第 358 轮实测 ✓）**
//
// **∴ 本工具做什么 ✗**：**通过 CDP 对**每个新文档**注入一小段脚本**✗
//   **∴ 它**只在**键不存在时**设 `"0"`** ✓
//     ⇒ **∴ 于是**：**无偏好 ⇒ 设 "0" ⇒ 内核加载** ✓
//       ＋ **∴ 而** `browser-render-switch` **自己设的 "1" 不被覆盖** ✓
//         （**∴ 那**条判据要测**服务端模式** ✓）★**** ✓✓
//
// 用法：CDP_PORT=9490 node scripts/cdp-seed-client-render.mjs
const port = process.env.CDP_PORT || "9490";
const KEY = "yanshi.serverRender";
const source = `try { if (localStorage.getItem(${JSON.stringify(KEY)}) === null) localStorage.setItem(${JSON.stringify(KEY)}, "0"); } catch (e) {}`;

let list;
try {
  list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error(`✗ 连不上 CDP（127.0.0.1:${port}）⇒ 不注入（${String(error)}）`);
  process.exit(0); // **∴ 不让判据因此失败 ✗**：没 CDP 时**静默跳过** ✓
}
const targets = list.filter((t) => t.type === "page");
if (targets.length === 0) {
  console.log("  · 没有 page 目标 ⇒ 跳过注入（浏览器判据自己会新建页面 ✓）");
  process.exit(0);
}
let seeded = 0;
for (const t of targets) {
  try {
    const ws = new WebSocket(t.webSocketDebuggerUrl);
    const pending = new Map();
    let seq = 0;
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(e.data);
      if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    });
    await new Promise((r) => ws.addEventListener("open", r));
    const send = (method, params = {}) => new Promise((res) => {
      const id = ++seq;
      pending.set(id, res);
      ws.send(JSON.stringify({ id, method, params }));
    });
    await send("Page.enable");
    // **★ 关键：对所有**新文档**生效 ✗ ★**（**∴ 于是**判据自己导航时也带上 ✓）
    const r = await send("Page.addScriptToEvaluateOnNewDocument", { source });
    if (!r || !r.error) seeded += 1;
    // **★ 双保险（**第 379 轮 ✓）：**再对**当前页面**直接设一次 ✗ ★**
    //   **∴ 为什么 ✗**：**实测（**第 379 轮 ✓）**：**`addScriptToEvaluateOnNewDocument`
    //     **对**之后的新文档**生效**✗（**∴ 单独测过：`seedProbe` = "yes" ✓）
    //       ⇒ **∴ 但**判据跑时**内核仍 false** ✓
    //         ⇒ **∴ 所以**怀疑**判据那一刻**当前页面**没被覆盖** ✓
    //           ⇒ **∴ 加这一步**代价极小**✗ ⇒ **∴ 而**它**覆盖**「**当前文档**」这一路 ✓ ★**** ✓✓
    //   **∴ 先导航到 about:blank 之外的页面才有效 ✗**：**`about:blank` **没有** localStorage** ✓
    //     ⇒ **∴ 所以**这一步**在**空页面**上**会失败**✗ ⇒ **∴ 用 try 包住** ＋ **不报错** ✓
    try {
      await send("Runtime.evaluate", { expression: source, returnByValue: true });
    } catch (error) {
      // **∴ 当前页面没有 localStorage（**如 about:blank ✓）⇒ **∴ 无害** ✓
    }
    ws.close();
  } catch (error) {
    console.error(`  · 某个目标注入失败（不影响其它）：${String(error).slice(0, 80)}`);
  }
}
console.log(`  ✓ 已为 ${seeded} 个页面目标预置「客户端渲染」偏好（只在键不存在时设 "0" ✓）`);
