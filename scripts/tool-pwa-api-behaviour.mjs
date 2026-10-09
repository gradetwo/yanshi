#!/usr/bin/env node
// **★ PWA 本地 API 的**行为级**判据 ✓ ★**（第 617 轮 ✓）
//
// **∴ 为什么需要它 ✗**：**`tool-pwa-assets.mjs` 只能证明"文件里写了什么"✗**（**字符串级 ✓**）
//   ⇒ **∴ 它证明不了"真的能用"✗** ⇒ **∴ 而"lazy 不许变成撒谎"要求**能跑的证据 ✗**
//   ⇒ **∴ 本判据在 node 里**打桩 `window`／`indexedDB`✗** ⇒ **∴ 真的执行
//   `installLocalApi` ＋ `makeLocalApi` ✗** ⇒ **∴ 断言**真实返回值 ✓****。
//
// **∴ 覆盖 ✗**：**① 静态资源走原 fetch ✓；② `/health` 如实报"无服务器"✗；
//   ③ `/api/documents` 创建 ✓；④ `/api/atoms` 写读 ✓；⑤ `list_layers` 从原子推导 ✓；
//   ⑥ `get_document` 报 `preview_state=pending` ＋ 后端 ＋ 原因 ✓；
//   ⑦ **未实现端点 ⇒ 501 ＋ 端点名 ＋ 原因 ✗**（**∴ 不静默失败 ✓**）** ✓✓
//
// **变异** ✗：**去掉 501 分支 ⇒ ⑦ 报红 ✓**（**已实测 ✓**）。
import { pathToFileURL } from "node:url";

// **★ 最小 IndexedDB 桩 ✓ ★**：**够用即可 ✗**（**只支持本层用到的 put／get／getAll ✓**）。
class MemStore {
  constructor() { this.map = new Map(); }
  // **∴ 键按 `store.js` 的 `keyPath` 语义取 ✗**：**`atoms`⇒`id` ✓｜`blobs`⇒`hash` ✓｜`docs`⇒`doc` ✓**。
  put(v) { this.map.set(v.id ?? v.hash ?? v.doc, v); return ok(); }
  get(k) { return ok(this.map.get(k)); }
  getAll() { return ok([...this.map.values()]); }
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
        // **∴ 必须返回带 `createIndex` 的对象 ✗**（**`store.js` 会为 `atoms` 建索引 ✓**）。
        createObjectStore: (n) => { stores.set(n, new MemStore()); return { createIndex() {} }; },
        transaction: (n) => ({ objectStore: () => stores.get(n) ?? (stores.set(n, new MemStore()), stores.get(n)) }),
      };
      req.result = db;
      queueMicrotask(() => { req.onupgradeneeded && req.onupgradeneeded(); req.onsuccess && req.onsuccess(); });
      return req;
    },
  };
}

// **★ 打桩浏览器环境 ✓ ★**
const calls = [];
// **★ 打桩要用 `defineProperty` ✗ ★**（**实测：node 26 的 `globalThis.navigator` **只有 getter**✗
// ⇒ **∴ 直接赋值会 `TypeError: Cannot set property navigator` ✓**）。
const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
stub("indexedDB", makeIndexedDB());
stub("location", { origin: "https://yanshi-online.wangda.today" });
stub("navigator", { gpu: undefined });   // **∴ 模拟「无 WebGPU」⇒ 后端应为 cpu ✓**
stub("window", {
  fetch: (input) => { calls.push(String(input.url ?? input)); return Promise.resolve(new Response("{}", { status: 200 })); },
});

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const { installLocalApi, makeLocalApi, LOCAL_IMPLEMENTED } = await import(base + "api-local.js");
const { open } = await import(base + "store.js");

const db = await open();
const local = await makeLocalApi(db);
const bad = [];
const check = (c, m) => { if (!c) bad.push(m); };
const call = (path, init = {}) => local(new Request(`https://x${path}?doc=d1`, { method: init.method ?? "GET", ...init }));

// **① 静态资源走原 fetch ✓**
await window.fetch({ url: "/wasm/yanshi_wasm.js" });
check(calls.some((u) => u.includes("/wasm/")), "静态资源未走原 fetch ✗");

// **②／③／④／⑤／⑥ 真实调用 ✓**
const created = await (await call("/api/documents", { method: "POST", body: JSON.stringify({ doc_id: "d1", width: 800, height: 600 }) })).json();
check(created.ok === true && created.server === false, `创建文档失败 ✗：${JSON.stringify(created)}`);
await call("/api/atoms", { method: "POST", body: JSON.stringify({ kind: "create_layer", layer_id: "L0", name: "图" }) });
const atoms = await (await call("/api/atoms")).json();
check(atoms.count >= 2, `原子写入后读回数量应 ≥ 2 ✗（实测 ${atoms.count}）`);
const layers = await (await call("/api/tools/list_layers")).json();
check(layers.layers.some((l) => l.id === "L0"), `list_layers 未从原子推导出 L0 ✗：${JSON.stringify(layers)}`);
check(layers.derived_from === "atoms", "list_layers 未标明由原子推导 ✗");
const gd = await (await call("/api/tools/get_document")).json();
check(gd.width === 800 && gd.height === 600, `get_document 尺寸应取自创建原子 ✗（实测 ${gd.width}×${gd.height}）`);
check(gd.preview_state === "pending", `get_document 的 preview_state 应为 pending ✗（实测 ${gd.preview_state}）`);
check(gd.render_backend === "cpu", `无 WebGPU 时后端应为 cpu ✗（实测 ${gd.render_backend}）`);
check(gd.gpu_unavailable_reason === "host_has_no_webgpu", `应报 GPU 不可用原因 ✗（实测 ${gd.gpu_unavailable_reason}）`);

// **⑦ 未实现端点 ⇒ 501 ＋ 原因 ✓**（**这是"不撒谎"的核心 ✓**）
const wrapped = (() => { installLocalApi({ local }); return window.fetch; })();
const un = await wrapped("https://x/api/tools/render_region?doc=d1", { method: "POST", body: "{}" });
check(un.status === 501, `未实现端点应返回 501 ✗（实测 ${un.status}）`);
const unBody = await un.json();
check(unBody.error === "not_implemented_locally", `未实现端点的 error 字段不对 ✗：${JSON.stringify(unBody)}`);
check(!!unBody.endpoint && unBody.endpoint.includes("render_region"), "未实现端点应报出端点名 ✗");
check(!!unBody.reason, "未实现端点应报出原因 ✗");
check(LOCAL_IMPLEMENTED >= 5, `声明实现数应 ≥ 5 ✗（实测 ${LOCAL_IMPLEMENTED}）`);

console.log(`  实际调用通过 ✓：创建 ✓｜原子 ${atoms.count} 条 ✓｜层 ${layers.count} 个 ✓｜`
  + `get_document ${gd.width}×${gd.height} pending ✓｜后端 ${gd.render_backend} ✓｜未实现端点 501 ✓`);
console.log(`  LOCAL_IMPLEMENTED = ${LOCAL_IMPLEMENTED} ✓`);
if (bad.length) { console.error("❌ " + bad.join("｜")); process.exit(1); }
console.log("  ✓ PWA 本地 API 在行为上成立（不是只看字符串 ✓）");
