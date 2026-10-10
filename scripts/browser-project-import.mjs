//! **★ 工程包**导出 ⇒ 导入**的端到端判据 ✗ ★**（第 73 轮 ✓；
//! **`yanshi-pwa-online-fix2.patch` 的**未覆盖项**换来的 ✓**）。
//!
//! **∴ 为什么需要它 ✗ ★**：**那份补丁**给 `api-local.js` **补上了**
//!   **`/api/documents/import` 的三步上传协议**✗
//!     （**∴ 原来**前端调 `?begin=1`**直接 throw `endpoint_not_local`**✗
//!       ⇒ **∴ 于是**：**导入工程文件**在 PWA 里**完全不可用 ✓）
//!   ⇒ **∴ 而**补丁自己的报告把"**真浏览器端到端 ✓"**列为**未覆盖** ✓**** ✓✓
//!   ⇒ **★ 所以**：**这一条判据**就是**那一项的落地 ✓ ★**** ✓✓
//!
//! **∴ 它验什么（**四步 ✓）★**：
//!   **∴ ①** **先**在页面里**建一个文档并画一笔**✗（**∴ 于是一定有**原子与 blob ✓）** ✓✓
//!   **∴ ②** **调** `export_project`**✗ ⇒ **∴ 拿到**工程包 JSON** ✓**** ✓✓
//!   **∴ ③** **按前端的**三步协议**把那个 JSON**导入回去**✗
//!     （**`?begin=1` ⇒ `?upload=<id>&offset=<n>` ⇒ `?upload=<id>&finish=1` ✓）** ✓✓
//!   **∴ ④** **对照 ✗**：**导入的**原子数**必须**等于**导出时的原子数**✗
//!     ＋ **返回** `ok:true`**✗ ＋ **新文档**真的能**列出原子 ✓**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/browser-project-import.mjs`

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const __tempPaths = [];
function trackTemp(path) { __tempPaths.push(path); return path; }
function __cleanupTemp() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let left = 0;
    for (const path of __tempPaths) {
      try { rmSync(path, { recursive: true, force: true }); } catch { left += 1; }
    }
    if (left === 0) return;
    try { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200); } catch { /* 忽略 */ }
  }
}
process.on("exit", __cleanupTemp);
for (const __signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(__signal, () => { __cleanupTemp(); process.exit(1); });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

// **∴ 用**仓内的静态服务**✗（**∴ 不**再自己写一份 ✓）** ✓✓
const PORT = 8813;
const server = spawn("node", ["scripts/serve-web.mjs", String(PORT), "web"], { stdio: "ignore" });
const base = `http://127.0.0.1:${PORT}`;
for (let i = 0; i < 60; i += 1) {
  try { if ((await fetch(`${base}/index.html`)).ok) break; } catch { /* 还没起来 */ }
  await sleep(250);
}

const cdpPort = 9500 + Math.floor(Math.random() * 80);
const profile = trackTemp(`/tmp/proj-import-chrome-${cdpPort}`);
const chrome = spawn(process.env.CHROME_BIN || "chromium", [
  "--headless=new", `--remote-debugging-port=${cdpPort}`, "--no-sandbox",
  "--disable-dev-shm-usage", "--window-size=1280,900", `--user-data-dir=${profile}`, "about:blank",
], { stdio: "ignore" });

let targets = null;
for (let i = 0; i < 80; i += 1) {
  await sleep(300);
  try {
    targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
    if (targets && targets.length) break;
  } catch { /* 还没起来 */ }
}
if (!targets || !targets.length) {
  console.error("❌ chromium 未就绪 ⇒ 本次实测无效");
  chrome.kill(); server.kill();
  process.exit(2);
}
const page = targets.find((t) => t.type === "page") || targets[0];
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const resolve = pending.get(message.id);
    pending.delete(message.id);
    resolve(message.result || message.error || {});
  }
});
await new Promise((resolve) => socket.addEventListener("open", resolve));
const send = (method, params) => {
  const id = nextId++;
  socket.send(JSON.stringify({ id, method, params: params || {} }));
  return new Promise((resolve) => pending.set(id, resolve));
};
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", {
    expression, returnByValue: true, awaitPromise: true,
  });
  if (result.exceptionDetails) {
    return "THREW:" + (result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  }
  return result.result ? result.result.value : undefined;
};
await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
await send("Network.setCacheDisabled", { cacheDisabled: true });

// **∴ 打开 PWA ✗**（**∴ 它是**localOnly ⇒ 一律本地 ✓）** ✓✓
await send("Page.navigate", { url: `${base}/index.html` });
for (let i = 0; i < 120; i += 1) {
  await sleep(250);
  if (await evaluate("document.readyState === 'complete' && !!document.getElementById('board')") === true) break;
}
await sleep(3000);
const stats = await evaluate("JSON.stringify(window.yanshiStats || {})");
console.log(`  页面状态：${String(stats).slice(0, 160)}`);

// **★ ① + ② 建文档、画一笔、导出工程包 ✗ ★**
const setup = await evaluate(`(async () => {
  const tool = async (name, args) => {
    const res = await fetch("/api/tools/" + name, { method: "POST",
      headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
    return await res.json();
  };
  try {
    // **★ 用**页面已有的文档**✗ ★**（第 73 轮 ✓；**∴ 我**第一版踩到了 ✓）：
    //   **∴ PWA 打开时**已经建了默认文档**✗ ⇒ **∴ 再** create_document
    //     ⇒ **∴ 回**"**文档已存在，重复 create_document ✓"（'ok:false' ✓）
    //       ⇒ **∴ 我**那条断言**是**多余的** ✓**** ✓✓
    const docId = (window.yanshiStats || {}).docId || null;
    const listed = await tool("list_layers", {});
    const layers = (listed && listed.layers) || [];
    let layerId = layers.length ? (layers[0].layer_id || layers[0].id) : null;
    let layerMade = null;
    if (!layerId) {
      layerMade = await tool("create_layer", { name: "crit" });
      layerId = layerMade && (layerMade.layer_id || layerMade.id) || null;
    }
    const stroke = await tool("draw_stroke", {
      layer_id: layerId, size: 24, color: { r: 17, g: 34, b: 51, a: 255 },
      points: [[40, 40, 0.8], [90, 70, 0.9], [150, 120, 1.0]],
    });
    const exported = await tool("export_project", {});
    // **★ 导出回的是**blob URL** ✗ ★**（**∴ 所以**必须去**取文本 ✓）** ✓✓
    let projectText = null;
    if (exported && typeof exported.url === "string") {
      try { projectText = await (await fetch(exported.url)).text(); } catch (error) {
        projectText = null;
      }
    }
    const head = await tool("get_log", {});
    return JSON.stringify({
      docId, layerId, layerMade: layerMade && layerMade.ok,
      layers: layers.length,
      strokeOk: !!(stroke && stroke.ok), strokeReason: (stroke && stroke.reason) || null,
      exportOk: !!(exported && exported.ok),
      exportKeys: exported ? Object.keys(exported).slice(0, 14) : [],
      atoms: exported && exported.atoms,
      exportDocId: exported && exported.doc_id,
      bytes: exported && exported.bytes,
      projectText,
      headAtoms: head && Array.isArray(head.atoms) ? head.atoms.map((a) => a.kind) : null,
    });
  } catch (error) { return JSON.stringify({ error: String(error && error.message || error) }); }
})()`);
let setupValue = null;
try { setupValue = JSON.parse(setup); } catch { /* 见下 */ }
if (!setupValue) {
  console.error("❌ setup 失败：" + String(setup).slice(0, 300));
  socket.close(); chrome.kill(); server.kill();
  process.exit(1);
}
console.log(`  导出：ok=${setupValue.exportOk}｜键=${(setupValue.exportKeys || []).join(",")}`);
// **∴ `localOnly` 模式下**文档 id 不在 `yanshiStats.docId`**✗
//   （**∴ 它**由内核持有 ✓）⇒ **∴ 所以**这里改看**导出结果里的 `doc_id`**✓
check(typeof setupValue.exportDocId === "string",
  "导出必须报出它属于哪个文档（**`doc_id` 字段 ✓）", String(setupValue.exportDocId));
check(setupValue.strokeOk === true,
  "页面里必须能画一笔（**`draw_stroke` ✓）",
  String(setupValue.strokeReason || setupValue.strokeOk));
console.log(`  文档=${setupValue.docId}｜图层=${setupValue.layers}｜原子种类=${(setupValue.headAtoms || []).join(",")}`);
check(setupValue.exportOk === true, "页面里必须能导出工程包", String(setupValue.exportOk));

const projectText = setupValue.projectText
  || (setupValue.project ? JSON.stringify(setupValue.project) : null);
if (!projectText) {
  console.error("❌ 导出的工程包取不到 ⇒ 无法做导入验证（原始：" + String(setupValue.raw).slice(0, 200) + "）");
  socket.close(); chrome.kill(); server.kill();
  process.exit(1);
}
let exportedAtoms = 0;
try {
  const parsed = JSON.parse(projectText);
  exportedAtoms = Array.isArray(parsed.atoms) ? parsed.atoms.length : 0;
} catch { /* 见下 */ }
console.log(`  工程包大小：${projectText.length} B｜原子数：${exportedAtoms}`);
check(exportedAtoms > 0, "工程包里必须有原子", String(exportedAtoms));

// **★ ③ + ④ 按前端的**三步协议**导入回去 ✗ ★**
const imported = await evaluate(`(async () => {
  const text = ${JSON.stringify(projectText)};
  const bytes = new TextEncoder().encode(text);
  const json = (r) => r.json();
  try {
    const begun = await json(await fetch("/api/documents/import?begin=1", { method: "POST" }));
    if (!begun || begun.ok !== true) return JSON.stringify({ step: "begin", begun });
    const uploadId = begun.upload_id;
    const chunk = 64 * 1024;
    let offset = 0;
    while (offset < bytes.length) {
      const slice = bytes.slice(offset, Math.min(offset + chunk, bytes.length));
      const res = await fetch("/api/documents/import?upload=" + encodeURIComponent(uploadId)
        + "&offset=" + offset, { method: "POST", body: slice });
      const body = await json(res);
      if (!body || body.ok !== true) return JSON.stringify({ step: "upload", body, offset });
      if (body.received !== offset + slice.length) {
        return JSON.stringify({ step: "upload", offsetMismatch: body.received, offset });
      }
      offset += slice.length;
    }
    const finishUrl = "/api/documents/import?upload=" + encodeURIComponent(uploadId)
      + "&finish=1&doc_id=" + encodeURIComponent("crit-imported");
    const done = await json(await fetch(finishUrl, { method: "POST" }));
    let listed = null;
    if (done && done.ok === true) {
      // **∴ 用**刚导入的文档**换一次 token 并列原子 ✗（**∴ 证明**它**真的落库了 ✓）** ✓✓
      const doc = await json(await fetch("/api/documents", { method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: done.doc_id, width: 256, height: 256 }) }));
      const url = "/api/atoms?doc=" + encodeURIComponent(done.doc_id)
        + "&token=" + encodeURIComponent((doc && doc.token) || "");
      listed = await json(await fetch(url));
    }
    return JSON.stringify({
      ok: done && done.ok === true,
      doc_id: done && done.doc_id,
      atoms: done && done.atoms,
      skipped: done && done.skipped,
      blobs: done && done.blobs,
      note: done && done.note,
      reason: done && done.reason,
      listedCount: listed && Array.isArray(listed.atoms) ? listed.atoms.length : null,
    });
  } catch (error) { return JSON.stringify({ error: String(error && error.message || error) }); }
})()`);
let importValue = null;
try { importValue = JSON.parse(imported); } catch { /* 见下 */ }
if (!importValue) {
  console.error("❌ 导入调用失败：" + String(imported).slice(0, 300));
  socket.close(); chrome.kill(); server.kill();
  process.exit(1);
}
console.log(`  导入：ok=${importValue.ok}｜doc=${importValue.doc_id}｜atoms=${importValue.atoms}`
  + `｜skipped=${importValue.skipped}｜blobs=${importValue.blobs}`
  + `｜落库原子=${importValue.listedCount}`);

check(importValue.ok === true,
  "三步协议必须成功（**`?begin` ⇒ `?upload` ⇒ `?finish` ✓）",
  importValue.reason || String(importValue.step || ""));
check(typeof importValue.doc_id === "string" && importValue.doc_id.length > 0,
  "必须回一个新的文档 id", String(importValue.doc_id));
check(importValue.atoms === exportedAtoms,
  "**导入的原子数必须**等于**导出时的原子数**（**∴ 端到端一致 ✓）",
  `导出 ${exportedAtoms} ⇒ 导入 ${importValue.atoms}`);
check(typeof importValue.listedCount === "number" && importValue.listedCount > 0,
  "**导入后的文档必须真的能列出原子**（**∴ 不是只回了个 ok ✓）",
  String(importValue.listedCount));

socket.close();
chrome.kill();
server.kill();
await sleep(400);

console.log("");
if (failures.length) {
  console.error(`  结论：工程包导入**未达预期** ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ 导出 ⇒ 三步导入 ⇒ 原子数一致 ⇒ 落库可列出（**端到端成立 ✓）");
process.exit(0);
