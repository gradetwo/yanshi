#!/usr/bin/env node
// **★ 本地 blob GC 的引用闭包必须与服务端同口径 ✓ ★**（第 1440 轮审计 ✓）
//
// **∴ 缺陷 ✗**：`collect_garbage`／`blob_gc`／`export_project` 三处原来**只看**
//   `payload.bitmap.blob_hash` ✗ ⇒ `import_image` 原子的**顶层** `payload.blob_hash`
//   **隐形** ✗ ⇒ **∴ `confirm:true` 会把正在使用的图删掉** ✗（**真数据丢失 ✓**）
//   ⇒ 重启后 blob 没了 ⇒ 图消失 ✗
// **∴ 修法 ✓**：`p2BlobRefsOfAtoms` —— **递归**收 `blob_hash`／`blob` 键下的 `sha256:` 串 ✓
//   （**与服务端 `atom.rs::collect_refs` 同口径 ✓**，**同一事实只写一处 ✓**）
//
// **∴ 本判据断的是结构量 ✗**（**数量／集合／删没删 ✓**）—— **不是像素 ✓**（内核 ① ✓）
// **∴ 四层齐 ✓**：实现（helper ✓）｜输入触发（**两种形状各造一条引用 ✗：顶层 ＋ 嵌套 ✓**）｜
//   断言（下面 8 条 ✓）｜变异（**把 helper 换回窄扫描 ⇒ ②③④⑥ 必红 ✓**，**见下 ✓**）
//
// **变异配方 ✓**（**python 精确替换 ✓，改 web/api-local.js ⇒ 跑 ⇒ 还原 ⇒ 同步 assets ✓**）：
//   把 `for (const a of atoms || []) walk(a && a.payload, null);`
//   换成   `for (const a of (atoms || []).filter(x => x && x.payload && x.payload.bitmap)) walk({ blob_hash: a.payload.bitmap.blob_hash }, null);`
//   ⇒ **顶层引用看不见** ⇒ orphan 集合错 ⇒ **②③④⑥ 红 ✗**（**已实测 ✓**）
process.on("uncaughtException", (e) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(e).slice(0, 200)}`);
  process.exit(3);
});
import { pathToFileURL } from "node:url";

// **★ 最小 IndexedDB 桩 ✓**（照 `tool-pwa-api-behaviour.mjs` ✓；**本判据多要求一件：`delete` ✗**
//   —— `collect_garbage{confirm:true}` 真删要调 `objectStore.delete` ⇒ 桩缺它会 TypeError ✗）
class MemStore {
  constructor() { this.map = new Map(); }
  put(v) { this.map.set(v.id ?? v.hash ?? v.doc, v); return ok(); }
  get(k) { return ok(this.map.get(k)); }
  getAll() { return ok([...this.map.values()]); }
  delete(k) { this.map.delete(k); return ok(); }
  createIndex() {}
}
const ok = (result) => { const r = { result, onsuccess: null, onerror: null }; queueMicrotask(() => r.onsuccess && r.onsuccess()); return r; };
function makeIndexedDB() {
  const stores = new Map();
  return {
    open() {
      const req = { result: null, onupgradeneeded: null, onsuccess: null, onerror: null };
      const db = {
        objectStoreNames: { contains: (n) => stores.has(n) },
        createObjectStore: (n) => { stores.set(n, new MemStore()); return { createIndex() {} }; },
        transaction: (n) => {
          const names = Array.isArray(n) ? n : [n];
          const t = { objectStore: (name) => {
            if (!names.includes(name)) throw new Error(`事务没有包含 store「${name}」`);
            if (!stores.has(name)) stores.set(name, new MemStore());
            return stores.get(name);
          } };
          setTimeout(() => { if (typeof t.oncomplete === "function") t.oncomplete(); }, 0);
          return t;
        },
      };
      req.result = db;
      queueMicrotask(() => { req.onupgradeneeded && req.onupgradeneeded(); req.onsuccess && req.onsuccess(); });
      return req;
    },
  };
}
const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
stub("indexedDB", makeIndexedDB());
stub("location", { origin: "https://x.test", href: "https://x.test/" });
stub("navigator", { gpu: undefined });
stub("window", { fetch: () => Promise.resolve(new Response("{}", { status: 200 })) });

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const { makeLocalApi } = await import(base + "api-local.js");
const { open, putBlob, getBlob, tx, wrap } = await import(base + "store.js");

const db = await open();
const local = await makeLocalApi(db);
const bad = [];
const check = (c, m) => { if (!c) bad.push(m); console.log(`  ${c ? "✓" : "✗"} ${m}`); };
const call = (path, init = {}) =>
  local(new Request(`https://x${path}?doc=g1`, { method: init.method ?? "GET", ...init }));

// —— 造文档（create_document ＋ create_layer ⇒ import_image 有活层可落 ✓）
await call("/api/documents", { method: "POST", body: JSON.stringify({ doc_id: "g1", width: 64, height: 64 }) });
await call("/api/atoms", { method: "POST", body: JSON.stringify({ kind: "create_layer", layer_id: "LA", name: "层" }) });

// —— 种三个 blob：**A 被顶层形状引用｜B 被嵌套形状引用｜C 无人引用** ✓
// **∴ 两种形状都要有 ✗**：旧缺陷**只看** `payload.bitmap.blob_hash` ✗
//   ⇒ A（`import_image` 的**顶层** `payload.blob_hash` ✓）正是**会被误删**的那类 ✗
const hashA = "sha256:" + "a".repeat(64);
const hashB = "sha256:" + "b".repeat(64);
const hashC = "sha256:" + "c".repeat(64);
await putBlob(db, hashA, [1, 2, 3, 4]);
await putBlob(db, hashB, [5, 6, 7, 8]);
await putBlob(db, hashC, [9, 9]);

const imp = await (await call("/api/tools/import_image", {
  method: "POST", body: JSON.stringify({ blob_hash: hashA, layer_id: "LA", object_id: "oImg" }),
})).json();
check(imp.ok === true, `import_image 应成功（制造**顶层** blob_hash 引用 ✗）：${JSON.stringify(imp).slice(0, 120)}`);
await call("/api/atoms", { method: "POST", body: JSON.stringify({
  kind: "create_object", payload: { object_id: "oRas", layer_id: "LA", bitmap: { blob_hash: hashB } } }) });

// **① 试运行**：**只报告不真删 ✓**（`dry_run` ＋ `reclaimed_blobs=0`）
const dry = await (await call("/api/tools/collect_garbage", { method: "POST", body: JSON.stringify({}) })).json();
check(dry.dry_run === true && dry.reclaimed_blobs === 0,
  `不带 confirm 必须只试运行 ✗（实测 ${JSON.stringify(dry).slice(0, 120)}）`);

// **② 引用集合 = 2**：旧窄扫描只数得出 B ⇒ 会把**在用的 A** 报成孤儿 ✗
check(dry.referenced === 2,
  `referenced 应为 2（顶层 A ＋ 嵌套 B ✓）✗（实测 ${dry.referenced} —— 顶层引用隐形 ⇒ 正是要删在用图的口径 ✗）`);

// **③ 孤儿恰为 [C]**：**A 绝不在孤儿里 ✗**（**这条守住数据 ✓**）
check(JSON.stringify(dry.orphans) === JSON.stringify([hashC]),
  `orphans 必须恰为 [C] ✗（实测 ${JSON.stringify(dry.orphans)}）`);

// **④ 真删之后：A／B 仍在、C 消失**
const gc = await (await call("/api/tools/collect_garbage", { method: "POST", body: JSON.stringify({ confirm: true }) })).json();
check(gc.reclaimed_blobs === 1 && gc.dry_run === false, `confirm:true 应恰回收 1 个 ✗（实测 ${gc.reclaimed_blobs}）`);
check(!!(await getBlob(db, hashA)), "被 import_image（顶层 blob_hash）引用的 A 必须存活 ✗（被删＝数据丢失 ✓）");
check(!!(await getBlob(db, hashB)), "被 payload.bitmap.blob_hash 引用的 B 必须存活 ✗");
check((await getBlob(db, hashC)) === null, "无引用的 C 必须被回收 ✗");

// **⑤ blob_gc 盘点同口径**：无引用数恰为 0（删完 C 之后）
const bg = await (await call("/api/tools/blob_gc", { method: "POST", body: JSON.stringify({}) })).json();
check(bg.blobs === 2 && bg.referenced === 2 && bg.unreferenced === 0,
  `blob_gc 应报 blobs=2｜referenced=2｜unreferenced=0 ✗（实测 ${bg.blobs}/${bg.referenced}/${bg.unreferenced}）`);

// **⑥ export_project 的引用闭包同口径**：A／B 都嵌进工程包 ⇒ **skipped=0** ✗
//   （**旧版把 A 静默漏嵌 ✗ 且不记 skipped** ⇒ **∴ 工程包损坏而调用方无信号 ✓**）
const exRes = await call("/api/tools/export_project", { method: "POST", body: JSON.stringify({}) });
const exText = await exRes.text();
const ex = JSON.parse(exText);
check(ex.blobs_embedded === 2 && ex.blobs_skipped === 0,
  `export_project 应嵌 2 个、漏 0 个 ✗（实测 embedded=${ex.blobs_embedded} skipped=${ex.blobs_skipped}）`);
// **工程包里 A 的内容须与库里一致 ✓**（**embedded 数对≠内容对 ✗**：base64 坏了同样是 2 ✓）
//   ⇒ **∴ 取包文本**：响应只给计数 ＋ `blob:` URL ✗（**`browser-project-import` 同法取文本 ✓**）
//   ⇒ **本 Node 桩取不了 blob: URL ⇒ 判据前提不成立 ⇒ exit(3)**（**不是产品失败 ✗**，第 336 轮 ✓）
let projText = null;
if (typeof ex.url === "string" && ex.url.startsWith("blob:")) {
  try { projText = await (await fetch(ex.url)).text(); }
  catch (e) {
    console.log(`  ⊘ 不可判：本环境取不了 blob: URL（${String(e).slice(0, 80)}）——`);
    console.log("     引用数断言已跑过 ✓，但「包内含 A」需能读包文本的环境 ⇒ 按前提不成立 exit(3)");
    if (bad.length) { console.log(`  ❌ ${bad.length} 处不符`); process.exit(1); }
    process.exit(3);
  }
}
const b64A = projText && (projText.match(new RegExp('"' + hashA.replace(/[:]/g, "\\$&") + '":"([^"]+)"')) || [])[1];
check(!!b64A && Buffer.from(b64A, "base64").join(",") === (await getBlob(db, hashA)).join(","),
  "工程包必须含**顶层引用**的 A 且字节一致 ✗（旧版静默丢图 ⇒ 工程包缺内容 ✓）");

// **⑦ 递归口径与服务端同形**：任意深度、数组里的 `blob_hash` 也算引用 ✗
//   （**服务端 `collect_refs` 递归遍历 ✓** ⇒ 本地少一条腿就是**又一种窄扫描 ✓**）
const hashD = "sha256:" + "d".repeat(64);
await putBlob(db, hashD, [7]);
await call("/api/atoms", { method: "POST", body: JSON.stringify({
  kind: "create_object",
  payload: { object_id: "oNes", layer_id: "LA", data: { texture: [{ blob_hash: hashD }] } } }) });
const gc2 = await (await call("/api/tools/collect_garbage", { method: "POST", body: JSON.stringify({}) })).json();
check(gc2.referenced === 3 && JSON.stringify(gc2.orphans) === "[]",
  `嵌套于数组的 blob_hash 也必须算引用 ✗（实测 referenced=${gc2.referenced} orphans=${JSON.stringify(gc2.orphans)}）`);

if (bad.length) {
  console.log(`\n  ❌ ${bad.length} 处不符（本地 blob 引用闭包）`);
  process.exit(1);
}
console.log("\n  ✓ 本地 GC／盘点／导出三处的 blob 引用闭包与服务端同口径（递归收 blob_hash/blob）");
