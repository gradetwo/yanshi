#!/usr/bin/env node
// **`blob_gc` 的完整性**：正常工作量后**不该有孤儿 blob**（第 1571 轮）
//
// **⇒ 为什么写这条**：外部那次 `seq` 重复导致**整份文档打不开**（约 10000 个对象），
//   事后查明**三个嫌疑**都排除了，**根因未定** ⇒ 而**它的近亲**是"原子引用了不存在的 blob"
//   （`duplicate_layer` 的"引用的 blob 尚未上传"就是这一族）✓
//   ⇒ **所以**要一条**结构量**判据：**文档引用的 blob 必须在存储里**，
//     **且 `blob_gc` 不该把"被引用的"算成可回收的** ✓
//
// **⇒ 断三条**：
//   ① `blob_gc(dry_run)` 成功，且 `documents_considered ≥ 1` ✓
//   ② **`collectible` 里不许有 `active` 里的 hash** ✓（**同一批 blob 不能既活跃又可回收**）
//   ③ 正常工作量后 **`collectible` 为空** ✓（**孤儿 = 0**）
//
// **⇒ 退出码**：0 通过｜1 不符｜3 不可判
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-blob-gc-integrity.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args),
  });
  return reply.json();
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// **先做一批"会产生 blob"的正常操作**：笔触（位图）、选区、复制图层、再删掉
await call("create_layer", { layer_id: "L", name: "L" });
await call("brush_stroke", {
  layer_id: "L", brush: "100%_Opaque", points: [[10, 10, 1], [50, 50, 1]],
  size: 8, color: [40, 40, 40], preview: true,
});
await call("create_selection", {
  selection_id: "S", mode: "new",
  shape: { kind: "polygon", points: [[5, 5], [40, 5], [40, 40], [5, 40]] },
});
await call("duplicate_layer", { layer_id: "L", new_layer_id: "L2" });
await call("delete_layer", { layer_id: "L2" });

const report = await call("blob_gc", { ttl_days: 0, dry_run: true });
if (report?.ok !== true) {
  console.error(`  ⊘ 不可判：blob_gc 失败（${JSON.stringify((report?.context || {}).detail || "").slice(0, 120)}）`);
  process.exit(3);
}
// **列表在 `hashes` 下，`active`/`collectible` 是计数对象** ✓（实测形状：
//   `active: {blobs,bytes}`、`collectible: {blobs,bytes}`、`hashes: {active:[…], collectible:[…]}`）
const hashes = report.hashes || {};
const active = new Set(hashes.active || []);
const collectible = new Set(hashes.collectible || []);

// ① 它真的看过这份文档
if ((report.documents_considered ?? 0) >= 1) ok(`① documents_considered=${report.documents_considered} ✓`);
else bad(`① documents_considered=${report.documents_considered} ⇒ 它没看任何文档 ⇒ 后面两条无意义`);

// ② 同一批 blob 不能既活跃又可回收
const both = [...collectible].filter((h) => active.has(h));
if (both.length === 0) ok(`② 活跃与可回收**互斥** ✓（active ${active.size} 个、collectible ${collectible.size} 个）`);
else bad(`② 有 ${both.length} 个 blob **同时**在 active 与 collectible 里 ⇒ ${both.slice(0, 3).join(",")}`);

// ③ **跑一次真 GC ⇒ 孤儿归零，且活跃的那个还在** ✓
//   **⇒ 我第一版写的是**「正常工作量后孤儿必须为 0」✗ —— **那条是错的**：
//     一次带预览的笔触 + 复制再删图层 ⇒ **实测就有 2 个孤儿**（预览 PNG、被删图层的位图）
//     ⇒ **而 `blob_gc` 存在的意义正是回收它们** ⇒ **孤儿在 GC 之前是**正常**的** ✓
//   **⇒ ⇒ 所以**判据要断的是**GC 的效果**，不是"有没有孤儿" ✓
console.log(`  ⊙ 实测：GC 前 active ${active.size} 个、collectible ${collectible.size} 个`);
// **真回收要 `dry_run: false` **且** `confirm: true`** ✓（产品那条护栏很好：
//   「回收是不可逆的」⇒ 它拒绝只给 `dry_run: false` 的调用 ✓）
const run = await call("blob_gc", { ttl_days: 0, dry_run: false, confirm: true });
if (run?.ok !== true) {
  console.error(`  ⊘ 不可判：真跑 blob_gc 失败（${JSON.stringify((run?.context || {}).detail || "").slice(0, 100)}）`);
  process.exit(3);
}
const after = await call("blob_gc", { ttl_days: 0, dry_run: true });
const afterCollectible = new Set((after?.hashes || {}).collectible || []);
const afterActive = new Set((after?.hashes || {}).active || []);
if (afterCollectible.size === 0) ok(`③ 真跑一次 GC 后 **孤儿 = 0** ✓（回收了 ${collectible.size} 个）`);
else bad(`③ 真跑一次 GC 后仍有 ${afterCollectible.size} 个孤儿 ⇒ ${[...afterCollectible].slice(0, 3).join(",")}`);
// **活跃的必须还在**（GC 不许误删）
const lost = [...active].filter((h) => !afterActive.has(h));
if (lost.length === 0) ok(`④ 活跃 blob **一个没少** ✓（${active.size} ⇒ ${afterActive.size}）`);
else bad(`④ GC **误删了 ${lost.length} 个活跃 blob** ⇒ ${lost.slice(0, 3).join(",")}`);

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（blob 完整性）`); process.exit(1); }
console.log("\n  ✓ blob_gc 的完整性：互斥且无孤儿");
