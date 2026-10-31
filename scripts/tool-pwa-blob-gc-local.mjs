#!/usr/bin/env node
// **本地 `blob_gc` 必须真的按服务端语义工作**（2026-10-31）
//
// **∴ 缺陷 ✗**：PWA 离线模式下，诊断面板的「回收孤儿」「降冷历史」两个按钮
//   走的是本地 `blob_gc` ✓，而它**只盘点、不执行** ✗ ⇒ 前端看到 `ok:true`
//   ⇒ 报「已回收孤儿 ✓／已降冷历史级 ✓」✗（**静默空操作冒充成功** ✓）。
//   统计面板也显示 `?`：本地响应字段（blobs/referenced/unreferenced）与服务端
//   （active/orphan/collectible）**不同形** ✗。
//
// **∴ 判据（行为级 ✓：node 里真跑 `web/api-local.js`）**：
//   ① 试运行只报告、不删；
//   ② `dry_run:false` 缺 `confirm` ⇒ `invalid_argument`（与服务端同口径）；
//   ③ `demote` ⇒ `not_implemented_locally`，且**一个都不删**（不拿回收冒充降冷）；
//   ④ `confirm:true` 真删孤儿、活跃 blob 不动；
//   ⑤ 恒等式：`active.blobs + orphan.blobs === blobs`（字节同）；
//   ⑥ `ttl_days` 非法 ⇒ 400（不静默当缺省）。
//
// **∴ 变异 ✓**：让 confirm 分支不删 ⇒ ④ 红；让 demote 走 `ok:true` ⇒ ③ 红；
//   让扫描把活跃算成孤儿 ⇒ ④⑤ 红（实测过前两条）。
process.on("uncaughtException", (e) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(e).slice(0, 200)}`);
  process.exit(3);
});
import { pathToFileURL } from "node:url";

const { makeIndexedDB } = await import(pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-stub.mjs").href);

const stub = (name, value) =>
  Object.defineProperty(globalThis, name, { value, writable: true, configurable: true });
stub("indexedDB", makeIndexedDB());
stub("location", { origin: "https://x.test", href: "https://x.test/" });
stub("navigator", { gpu: undefined });
stub("window", { fetch: () => Promise.resolve(new Response("{}", { status: 200 })) });

const base = pathToFileURL(`${process.cwd()}/web/`).href;
const { makeLocalApi } = await import(base + "api-local.js");
const { open, putBlob, getBlob } = await import(base + "store.js");

const db = await open();
const local = await makeLocalApi(db);
const bad = [];
const check = (c, m) => { if (!c) bad.push(m); console.log(`  ${c ? "✓" : "✗"} ${m}`); };
const call = (path, init = {}) =>
  local(new Request(`https://x${path}?doc=gc1`, { method: init.method ?? "GET", ...init }));
const post = (path, body) => call(path, { method: "POST", body: JSON.stringify(body) });

// —— 造文档：1 个被引用的 blob（A）＋ 2 个孤儿（B、C）——
await post("/api/documents", { doc_id: "gc1", width: 64, height: 64 });
await post("/api/atoms", { kind: "create_layer", layer_id: "LA", name: "层" });
const hashA = "sha256:" + "a".repeat(64);
const hashB = "sha256:" + "b".repeat(64);
const hashC = "sha256:" + "c".repeat(64);
await putBlob(db, hashA, [1, 2, 3, 4]);
await putBlob(db, hashB, [5, 6]);
await putBlob(db, hashC, [7, 8, 9, 0, 1, 2]);
await post("/api/atoms", { kind: "create_object", payload: {
  object_id: "oA", layer_id: "LA", bitmap: { blob_hash: hashA } } });

// ① 试运行：只报告 ⇒ 数字对得上、blob 一个不少
const dry = await (await post("/api/tools/blob_gc", {})).json();
check(dry.ok === true && dry.dry_run === true,
  `不带 dry_run 必须缺省为试运行 ✗（实测 ${JSON.stringify(dry).slice(0, 120)}）`);
check(dry.blobs === 3 && dry.active.blobs === 1 && dry.orphan.blobs === 2,
  `试运行应报 3 个 blob（活跃 1／孤儿 2）✗（实测 ${dry.blobs}／${dry.active?.blobs}／${dry.orphan?.blobs}）`);
check(dry.active.blobs + dry.orphan.blobs === dry.blobs,
  `恒等式（数量）：active + orphan（${dry.active?.blobs}+${dry.orphan?.blobs}）应等于 blobs（${dry.blobs}）`);
check(dry.active.bytes + dry.orphan.bytes === dry.bytes,
  `恒等式（字节）：active + orphan（${dry.active?.bytes}+${dry.orphan?.bytes}）应等于 bytes（${dry.bytes}）`);
check(dry.active.bytes === 4 && dry.orphan.bytes === 8,
  `字节应活跃 4／孤儿 8 ✗（实测 ${dry.active?.bytes}／${dry.orphan?.bytes}）`);
check(dry.collectible.blobs === 2 && dry.ttl_days === 7 && dry.ttl_applied === false,
  `本地应报可回收 2、ttl_days 回显 7 且 ttl_applied:false ✗（实测 ${dry.collectible?.blobs}／${dry.ttl_days}／${dry.ttl_applied}）`);
check(!!(await getBlob(db, hashB)) && !!(await getBlob(db, hashC)),
  "试运行不得删除任何 blob ✗");

// ② 想删但没确认 ⇒ 400 invalid_argument（服务端同口径，不静默）
const noConfirmRes = await post("/api/tools/blob_gc", { dry_run: false });
const noConfirm = await noConfirmRes.json();
check(noConfirmRes.status === 400 && noConfirm.error_code === "invalid_argument",
  `dry_run:false 缺 confirm 应 400 invalid_argument ✗（实测 ${noConfirmRes.status}／${noConfirm.error_code}）`);

// ③ 降冷：本地无冷存储 ⇒ 明确 501，且不得顺手删孤儿
const demoteRes = await post("/api/tools/blob_gc", { dry_run: false, confirm: true, demote: true });
const demote = await demoteRes.json();
check(demoteRes.status === 501 && demote.error_code === "not_implemented_locally",
  `demote 应明确报 not_implemented_locally ✗（实测 ${demoteRes.status}／${demote.error_code}）`);
check(demote.demoted === undefined && demote.ok === false,
  `demote 失败时不得声称降冷成功 ✗（实测 demoted=${demote.demoted}、ok=${demote.ok}）`);
check(!!(await getBlob(db, hashB)) && !!(await getBlob(db, hashC)),
  "demote 失败时不得顺手删除孤儿 ✗");

// ⑥ ttl_days 非法 ⇒ 400（不静默当缺省）
const badTtlRes = await post("/api/tools/blob_gc", { ttl_days: -1 });
const badTtl = await badTtlRes.json();
check(badTtlRes.status === 400 && badTtl.error_code === "invalid_argument",
  `ttl_days:-1 应 400 invalid_argument ✗（实测 ${badTtlRes.status}／${badTtl.error_code}）`);

// ④ 真回收：删两个孤儿、活跃 A 保留
const run = await (await post("/api/tools/blob_gc", { dry_run: false, confirm: true })).json();
check(run.ok === true && run.dry_run === false && run.removed === 2 && run.freed_bytes === 8,
  `confirm:true 应真删 2 个／释放 8 字节 ✗（实测 ok=${run.ok} removed=${run.removed} freed=${run.freed_bytes}）`);
check(run.demoted === undefined,
  `没要降冷就不许出现 demoted 字段 ✗（实测 ${run.demoted}）`);
check(run.active.blobs + run.orphan.blobs === run.blobs,
  `恒等式（数量）：active=${run.active?.blobs} orphan=${run.orphan?.blobs} blobs=${run.blobs}（前两者之和必须等于后者）`);
check(!!(await getBlob(db, hashA)), "被原子引用的 A 必须存活 ✗（被删＝数据丢失）");
check((await getBlob(db, hashB)) === null && (await getBlob(db, hashC)) === null,
  "无引用的 B／C 必须被删除 ✗");

// ⑤ 再盘点：只剩 A，恒等式仍成立
const after = await (await post("/api/tools/blob_gc", {})).json();
check(after.blobs === 1 && after.orphan.blobs === 0 && after.unreferenced === 0,
  `回收后应只剩 1 个活跃 blob ✗（实测 ${after.blobs}／${after.orphan?.blobs}／${after.unreferenced}）`);
check(after.active.bytes + after.orphan.bytes === after.bytes,
  `恒等式（字节）：active=${after.active?.bytes} orphan=${after.orphan?.bytes} bytes=${after.bytes}（前两者之和必须等于后者）`);

if (bad.length) {
  console.log(`\n  ✗ ${bad.length} 处不符（本地 blob_gc 的回收／降冷／报告语义）`);
  process.exit(1);
}
console.log("\n  ✓ 本地 blob_gc 真回收孤儿、降冷诚实报不支持、报告与服务端同形");
