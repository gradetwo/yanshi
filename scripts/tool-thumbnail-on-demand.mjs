#!/usr/bin/env node
// **缩略图必须"按需生成"**判据（第 77 轮 ✓）—— 真实用户报告的直接护栏 ✓
//
// **为什么需要它** ✗：保存点（`export_project` ✓）**无条件**调用 `thumbnail(Doc256)` ✓，
//   而那次调用会**整幅渲染一遍** ✗ —— 实测（真实 4K ✓）**每次多余 ~1.07 s** ✗、
//   8K **~9 s** ✗，**画面零变化也照样付** ✓（用户原话："導出第二次仍 1.2 s" ✗）。
//   **∴ 修法**：用 `doc_thumbnail_is_current()` 判定 ✓ ⇒ 不最新才生成 ✓。
//   **∴ 修后实测**：4K 重复导出 **1280 → 85 ms（15× ✓）**、8K **9174 → 573 ms（15× ✓）**。
//
// **判据（四条 ✓，成对才算完整 ✗）**：
//   ① **首次必须真的生成** ✓（`thumbnail_renders >= 1` ✓）—— 防"从不生成"骗过下面两条 ✗；
//   ② **无变化 ⇒ 计数不得增长** ✓（主判据 ✓；**修复前必红** ✗）；
//   ③ **有变化 ⇒ 必须更新** ✓（**数量必须增长** ✓）—— 防"永不更新"这个**真正的撒谎** ✗；
//   ④ **导出产物必须不变** ✓（`bytes` 与 `blob_hash` 两次相同 ✓）—— 防"为了快而少装东西" ✗。
//
// **变异** ✗（打在**被judged的那一处** ✓）：
//   * 把保存点的"按需"判定去掉（回到无条件调用 ✓）⇒ **② 必红** ✓；
//   * 或把判定改成**恒真**（永不更新 ✓）⇒ **③ 必红** ✓。
//
// **不写死观测** ✓：比的是"**两次计数是否相同**"与"**变化后是否增长**"（语义边界 ✓），
//   不看耗时、不看具体毫秒 ✓。
//
// 用法：node scripts/tool-thumbnail-on-demand.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-thumbnail-on-demand.mjs <base-url>");
  process.exit(2);
}
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const doc = "thumb-od-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
})).json();
if (!created.token) {
  console.error("✗ 前置不成立：建不出文档 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const token = created.token;
const call = async (tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
// **结构性读数** ✓：`thumbnail_renders` 由 `get_document` 报出 ✓（与墙钟无关 ✓）。
const renders = async () => Number((await call("get_document", {})).thumbnail_renders ?? -1);

if ((await call("create_layer", { layer_id: "L1" })).ok !== true) {
  console.error("✗ 前置不成立：建不出图层");
  process.exit(2);
}
const stroke = await call("brush_stroke", {
  layer_id: "L1", object_id: "o1", brush: "100%_Opaque", size: 40,
  color: { r: 20, g: 140, b: 220, a: 255 }, preview: false,
  points: [[40, 120, 1], [280, 120, 1]],
});
if (stroke.ok !== true) {
  console.error("✗ 前置不成立：笔触未成功 ⇒ " + JSON.stringify(stroke).slice(0, 160));
  process.exit(2);
}

const e1 = await call("export_project", {});
const r1 = await renders();
const e2 = await call("export_project", {});
const r2 = await renders();
console.log(`  首次导出后 thumbnail_renders=${r1}｜无变化再导出后=${r2}｜bytes=${e1.bytes}/${e2.bytes}`);

// ③ **有变化**：新建图层并画一笔 ⇒ 缩略图**必须**重做 ✓
if ((await call("create_layer", { layer_id: "L2" })).ok !== true) {
  console.error("✗ 前置不成立：第二个图层建不出");
  process.exit(2);
}
const stroke2 = await call("brush_stroke", {
  layer_id: "L2", object_id: "o2", brush: "100%_Opaque", size: 60,
  color: { r: 230, g: 60, b: 40, a: 255 }, preview: false,
  points: [[40, 200, 1], [280, 200, 1]],
});
if (stroke2.ok !== true) {
  console.error("✗ 前置不成立：第二笔未成功 ⇒ " + JSON.stringify(stroke2).slice(0, 160));
  process.exit(2);
}
const e3 = await call("export_project", {});
const r3 = await renders();
console.log(`  改动后再导出 ⇒ thumbnail_renders=${r3}`);

// ① **首次必须真的生成** ✓（否则下面两条可以被"从不生成"骗过 ✗）
check(r1 >= 1, "首次保存必须**真的生成**缩略图（`thumbnail_renders >= 1`）",
  "r1=" + r1);
// ② **主判据** ✓：无变化 ⇒ 不得增长 ✓
check(r2 === r1, "**无变化时重复保存 ⇒ 缩略图渲染次数不得增长**（修复前必红）",
  `${r1} → ${r2}`);
// ③ **防撒谎** ✓：有变化 ⇒ 必须增长 ✓
check(r3 > r2, "**有变化之后再保存 ⇒ 缩略图必须更新**（防「永不更新」这个真正的撒谎）",
  `${r2} → ${r3}`);
// ④ **导出产物不得变** ✓
check(String(e1.blob_hash) === String(e2.blob_hash) && e1.bytes === e2.bytes,
  "两次导出的**内容寻址哈希与字节数**必须相同（不许为了快而少装东西）",
  `${String(e1.blob_hash).slice(7, 23)}/${e1.bytes} vs ${String(e2.blob_hash).slice(7, 23)}/${e2.bytes}`);

console.log("");
if (failures.length) {
  console.error(`结论：缩略图**没有按需生成** ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 缩略图按需生成（首次必生成、无变化不重做、有变化必更新、导出产物不变）");
process.exit(0);
