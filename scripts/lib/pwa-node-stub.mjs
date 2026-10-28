#!/usr/bin/env node
// **★ 共享的 node 侧「浏览器桩」✓ ★**（第 2 批从 `tool-pwa-api-behaviour.mjs` 原样抽出 ✓）：
//   **∴ 为什么要抽 ✗**：PWA 判据要在 node 里**真跑** `web/api-local.js` ⇒ 需要 `indexedDB` 桩 ✓
//   ⇒ **∴ 旧状 ✗**：桩只写在那一条判据里 ✓ ⇒ 第二条判据要么**抄一份**✗
//     （**内核 ②「同一事实只写一处」✗ ⇒ 抄两份一定漂移 ✓**）
//   ⇒ **∴ 现在**：两条判据 import 同一份 ✓ ⇒ **桩要改只改这里 ✓**。
//   **∴ 用法 ✓**：`const local = await bootPwaLocalApi({ localOnly: true })` ⇒ 直接 `local(new Request(…))` ✓。
import { pathToFileURL } from "node:url";

// —— 以下三件套（`MemStore`／`ok`／`makeIndexedDB`）自 `tool-pwa-api-behaviour.mjs` 逐字搬来 ✓
class MemStore {
  constructor() { this.map = new Map(); }
  // **∴ 键按各表 `keyPath` 取 ✗**：**`atoms`⇒`id` ✓｜`blobs`⇒`hash` ✓｜`docs`⇒`doc` ✓｜`prefs`⇒`key` ✓**。
  //   **★ 漏掉 `key` 的后果（第 六百六十二 轮实测）**：`prefs` 记录是 `{key, value}` ⇒
  //   `v.id ?? v.hash ?? v.doc` 全是 `undefined` ⇒ **每条偏好都写进同一个 `undefined` 槽 ✗**
  //   ⇒ `set_reference` 回 `ok:true` 而 `get("reference.blob_hash")` 恒 `undefined` ✗
  //   ⇒ **判据看到「没有参考图」** ⇒ **∴ 产品对、判据红** ⇒ **∴ 修桩，不迁就桩改产品 ✓**。
  put(v) { this.map.set(v.id ?? v.hash ?? v.doc ?? v.key, v); return ok(); }
  get(k) { return ok(this.map.get(k)); }
  delete(k) { this.map.delete(k); return ok(undefined); }
  getAll() { return ok([...this.map.values()]); }
  createIndex() {}
}
const ok = (result) => { const r = { result, onsuccess: null, onerror: null }; queueMicrotask(() => r.onsuccess && r.onsuccess()); return r; };
export function makeIndexedDB() {
  const stores = new Map();
  return {
    open() {
      const req = { result: null, onupgradeneeded: null, onsuccess: null, onerror: null };
      const db = {
        objectStoreNames: { contains: (n) => stores.has(n) },
        // **∴ 必须返回带 `createIndex` 的对象 ✗**（**`store.js` 会为 `atoms` 建索引 ✓**）。
        createObjectStore: (n) => { stores.set(n, new MemStore()); return { createIndex() {} }; },
        // **★ 桩必须模拟真 IndexedDB 的两个行为**（2026-10-20，本判据在 CI 上 exit 13 / 读回 0）：
        //   ① `transaction([a, b])` **接受数组**（真 API 就是这样；D-01 的单事务要用它）
        //      ⇒ 原来的桩只 `stores.get(n)` ⇒ 传数组时拿到 undefined ⇒ **另建一个以数组为 key 的 store**
        //        ⇒ 原子的 write 全进了那个假 store ⇒ 读回 0 笔触 ✗
        //   ② 事务是**异步提交**的，会触发 `oncomplete`
        //      ⇒ 原来的桩没有它 ⇒ 产品里 `await done` **永远不 resolve** ⇒ 判据挂死（Node exit 13）✗
        //   ⇒ 桩不完备会让「产品对、判据红」⇒ 所以我修桩，**不**迁就桩去改产品 ✓
        transaction: (n) => {
          const names = Array.isArray(n) ? n : [n];
          const storeFor = (name) => {
            if (!names.includes(name)) {
              throw new Error(`事务没有包含 store「${name}」（现有：${names.join(",")}）`);
            }
            if (!stores.has(name)) stores.set(name, new MemStore());
            return stores.get(name);
          };
          const t = { objectStore: (name) => storeFor(name) };
          // 真 API 在微任务量级内提交 ⇒ 用 setTimeout 0（`oncomplete` 属性那时已赋好）
          setTimeout(() => {
            if (typeof t.oncomplete === "function") t.oncomplete();
          }, 0);
          return t;
        },
      };
      req.result = db;
      queueMicrotask(() => { req.onupgradeneeded && req.onupgradeneeded(); req.onsuccess && req.onsuccess(); });
      return req;
    },
  };
}

/** **装配 ✓**：打桩 `indexedDB`／`location`／`navigator`／`window` ⇒ 返回本地 API 句柄 ✗
 *   （**`localOnly:true` ⇒ PWA 分支 ✓**（`window.__pwaLocalOnly === true` ⇒ 不代理、不读服务端日志 ✓）；
 *   **要测 C/S 代理的判据自己换 `globalThis.window` ✗ 并 stub `fetch` ✓**）。 */
export async function bootPwaLocalApi({ origin = "https://yanshi-local.test", localOnly = true } = {}) {
  const def = (name, value) =>
    Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
  def("indexedDB", makeIndexedDB());
  def("location", { origin, href: origin + "/" });
  def("navigator", { gpu: undefined });   // **模拟「无 WebGPU」⇒ 后端应为 cpu ✓**
  def("window", { __pwaLocalOnly: localOnly,
                   fetch: () => Promise.resolve(new Response("{}", { status: 200 })) });
  const webDir = pathToFileURL(process.cwd() + "/web/").href;
  const { makeLocalApi } = await import(webDir + "api-local.js");
  const { open } = await import(webDir + "store.js");
  return await makeLocalApi(await open());
}
