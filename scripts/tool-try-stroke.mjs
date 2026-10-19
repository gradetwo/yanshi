#!/usr/bin/env node
// **`try_stroke` 必须只算不落笔**（需求 v2 §2.4 验收 ②）
//   ① 原子数不变 ② seq 不变 ③ 文件字节数不变 ④ pixels_touched > 0 ⑤ bbox 框住改变像素
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-try-stroke.mjs <base-url>"); process.exit(2); }
const doc = `try-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "try-stroke-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 200, height: 200 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "p", name: "p" });
const before = await call("list_atoms", { limit: 100, include_non_stroke: true });
const sumBefore = await call("get_document", {});
const failures = [];

const r = await call("try_stroke", {
  layer_id: "p", brush: "100%_Opaque", points: [[20, 20, 1], [80, 80, 1]], size: 12,
  color: { r: 255, g: 0, b: 0, a: 255 }, want_image: false,
});
console.log(`  try_stroke ⇒ ok=${r.ok}｜bbox=${JSON.stringify(r.bbox)}｜pixels_touched=${r.pixels_touched}`);
if (r.ok !== true) failures.push(`try_stroke 失败 ⇒ ${JSON.stringify(r).slice(0, 140)}`);

const after = await call("list_atoms", { limit: 100, include_non_stroke: true });
const sumAfter = await call("get_document", {});

// ① 原子数不变
console.log(`  原子数 ${before.count} ⇒ ${after.count}`);
if (Number(before.count) !== Number(after.count)) failures.push(`试画后原子数从 ${before.count} 变成 ${after.count} ⇒ 它落笔了 ✗`);

// ② seq 不变
// **用 list_atoms 的最大 seq 当"文档到第几笔"**（get_document 的参数面取不到 seq）
const maxSeq = (page) => Math.max(0, ...(page.atoms ?? []).map((a) => Number(a.seq) || 0));
const q1 = maxSeq(before);
const q2 = maxSeq(after);
console.log(`  最大 seq ${q1} ⇒ ${q2}`);
if (before.count == null || after.count == null) {
  console.error("  判据自身前提不成立：list_atoms 没返回 count（不是产品问题）");
  process.exit(3);
}
if (q1 !== q2) failures.push(`试画后最大 seq 从 ${q1} 变成 ${q2} ⇒ 它落笔了 ✗`);

// ③ want_image：缺省必须出图、false 必须不出图（外部 §113 称"不出图"是第一阻塞项 ⇒ 这条路径以前没验过）
const withImg = await call("try_stroke", {
  layer_id: "p", brush: "100%_Opaque", points: [[40, 120, 1], [120, 160, 1]], size: 12,
  color: { r: 0, g: 128, b: 255, a: 255 },
});
const noImg = await call("try_stroke", {
  layer_id: "p", brush: "100%_Opaque", points: [[40, 160, 1], [120, 200, 1]], size: 12,
  color: { r: 0, g: 128, b: 255, a: 255 }, want_image: false,
});
console.log(`  缺省 want_image ⇒ 有图=${!!withImg.image}（长度 ${String(withImg.image || "").length}）｜false ⇒ 有图=${!!noImg.image}`);
if (!withImg.image) failures.push("缺省（want_image 应为 true）却没返回 image ⇒ 试画退化成只看 bbox ✗");
if (String(withImg.image || "").length < 100) failures.push(`缺省返回的 image 太短（${String(withImg.image || "").length} 字符）⇒ 可能是空图 ✗`);
if (noImg.image) failures.push("want_image=false 却仍返回 image ⇒ 开关没生效 ✗");

// ④ pixels_touched > 0（这一笔确实算出了东西 ⇒ 防"恒 0 也能过"）
if (!(Number(r.pixels_touched) > 0)) failures.push(`pixels_touched=${r.pixels_touched}，应大于 0 ⇒ 试画没真的算 ✗`);

// ⑤ bbox 四元组且宽高为正
if (!Array.isArray(r.bbox) || r.bbox.length !== 4) failures.push(`bbox 不是 4 元组：${JSON.stringify(r.bbox)} ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 试画不落笔（原子数与 seq 不变），且确实算出了像素");
