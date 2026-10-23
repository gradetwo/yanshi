#!/usr/bin/env node
// **★ 标注通道的 `channel` 必须报"实际生效"的那个，不许撒谎 ✓ ★**（第 1440 轮审计 ✓）
//
// **∴ 缺陷 ✗**：旧版 `p2bAnnChannel()` 只看 `localStorage` **存不存在** ✗ ⇒
//   `setItem` 被**配额**打回时异常被**静默吞掉** ✗ ⇒ 响应仍报 `channel:"localStorage"` ✗
//   ⇒ **∴ 调用方以为已持久化**，实际只活在内存 ⇒ **刷新即丢** ✗（**静默降级 ✓**）
// **∴ 修法 ✓**：`p2bAnnSave` 记录 `{persisted, reason}` ⇒ `p2bAnnChannel` 见上一次写失败
//   ⇒ 降级报 `memory` ✓；下一次成功写**自愈** ✓；并 `console.warn` 说出原因 ✓
//
// **∴ 断的全是结构量 ✓**（字段值／降级／自愈 ✓）—— **不是像素 ✓**（内核 ① ✓）
// **∴ 四层齐 ✓**：实现 ✓｜输入触发（**桩里让 setItem 抛 QuotaExceeded ✓**）｜
//   断言（下面 5 条 ✓）｜变异（**把 `p2bAnnChannel` 换回旧版 ⇒ ①③ 红 ✓**，配方见下 ✓）
//
// **变异配方 ✓**：把
//   `if (typeof localStorage !== "undefined" && !(last && last.persisted === false)) {`
//   换回旧版   `if (typeof localStorage !== "undefined") {`
//   ⇒ 配额失败后仍报 `localStorage` ⇒ **①③④ 必红 ✗**（**已实测 ✓**）
process.on("uncaughtException", (e) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(e).slice(0, 200)}`);
  process.exit(3);
});
import { pathToFileURL } from "node:url";

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

// **★ 可控 localStorage 桩 ✓**：`quotaOn` ⇒ `setItem` 抛 `QuotaExceeded` ✗（**制造缺陷输入 ✓**）
let quotaOn = true;
const lsBacking = new Map();
stub("localStorage", {
  getItem: (k) => (lsBacking.has(k) ? lsBacking.get(k) : null),
  setItem: (k, v) => {
    if (quotaOn) { const e = new Error("exceeded the quota"); e.name = "QuotaExceededError"; throw e; }
    lsBacking.set(k, v);
  },
});
// **捕获 console.warn ✓**（**降级必须可观测 ✗** —— 原因不许只存在于实现内部）
const warns = [];
const realWarn = console.warn;
console.warn = (...a) => { warns.push(a.join(" ")); realWarn(...a); };

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const { makeLocalApi } = await import(base + "api-local.js");
const { open } = await import(base + "store.js");

const db = await open();
const local = await makeLocalApi(db);
const bad = [];
const check = (c, m) => { if (!c) bad.push(m); console.log(`  ${c ? "✓" : "✗"} ${m}`); };
const call = (path, init = {}) =>
  local(new Request(`https://x${path}?doc=a1`, { method: init.method ?? "GET", ...init }));

await call("/api/documents", { method: "POST", body: JSON.stringify({ doc_id: "a1", width: 64, height: 64 }) });

// **① 配额打回 ⇒ channel 必须降级报 `memory` ✗**（**旧版报 `localStorage` ⇒ 撒谎 ✓**）
const ann = await (await call("/api/tools/create_annotation", {
  method: "POST",
  body: JSON.stringify({ type: "region", intent: "modify", target: { bbox: { x: 0, y: 0, w: 10, h: 10 } }, content: "这里暗一点" }),
})).json();
check(ann.ok === true, `create_annotation 本身应成功（内存里要有 ✓）：${JSON.stringify(ann).slice(0, 140)}`);
check(ann.channel === "memory",
  `setItem 被配额打回 ⇒ channel 必须如实报 memory ✗（实测 ${JSON.stringify(ann.channel)} —— 报 localStorage 就是撒谎 ✓）`);

// **② 降级要说得出原因 ✓**（console.warn 含「未能持久化」＋底层错误摘要）
check(warns.some((w) => w.includes("未能持久化") && w.includes("quota")),
  `持久化失败必须 console.warn 说出原因（含底层错误摘要 ✗）（实测 warn 条数 ${warns.length}）`);

// **③ 后续读请求同样报 memory**（**不许只在写响应里降级 ✓**）
const got = await (await call("/api/tools/get_annotation", {
  method: "POST", body: JSON.stringify({ annotation_id: ann.annotation_id }) })).json();
check(got.ok === true && got.channel === "memory",
  `失败状态下的读响应也必须报 memory ✗（实测 ${JSON.stringify(got.channel)}）`);

// **④ 自愈 ✓**：配额恢复后的下一次**写**成功 ⇒ channel 回到 localStorage ✓
quotaOn = false;
const upd = await (await call("/api/tools/update_annotation", {
  method: "POST",
  body: JSON.stringify({ annotation_id: ann.annotation_id, content: "改完了", status: "pending" }),
})).json();
check(upd.ok === true && upd.channel === "localStorage",
  `下一次成功写之后 channel 必须自愈回 localStorage ✗（实测 ok=${upd.ok} channel=${JSON.stringify(upd.channel)}）`);
check(lsBacking.has("yanshi:annotations:a1"), "成功写必须真的落到 localStorage ✓");

// **⑤ 无 localStorage 的环境 ⇒ 如实 memory ✓**（**第 430 轮那条：能力边界用字段说 ✗**）
delete globalThis.localStorage;
const ann5 = await (await call("/api/tools/create_annotation", {
  method: "POST",
  body: JSON.stringify({ type: "text", intent: "add", target: { bbox: { x: 1, y: 1, w: 2, h: 2 } }, content: "纯内存" }),
})).json();
check(ann5.ok === true && ann5.channel === "memory",
  `localStorage 不存在时 channel 必须报 memory（且标注仍可用）✗（实测 ${JSON.stringify(ann5.channel)}）`);

if (bad.length) {
  console.log(`\n  ❌ ${bad.length} 处不符（标注通道的诚实性）`);
  process.exit(1);
}
console.log("\n  ✓ 标注 channel 报「实际生效」的通道：配额失败即降级、说得出原因、成功写自愈");
