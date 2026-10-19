#!/usr/bin/env node
// **try_batch / commit_batch 必须闭环**（需求 v2 §三：一个部位通常 5~8 笔）
//   ① try_batch 不落笔（笔迹数不变）② 返回 3 个 attempt_ids
//   ③ commit_batch 按顺序贴回 ⇒ 笔迹数 +3、返回 3 个 atom_ids
//   ④ 整组标签跟着走（按 label 查到 3 条）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-batch.mjs <base-url>"); process.exit(2); }
const doc = `tb-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "batch-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 300, height: 300 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "p", name: "p" });
const failures = [];
const LABEL = "BATCH-EYE";
const strokes = [0, 1, 2].map((i) => ({
  brush: "100%_Opaque",
  points: [[20 + i * 80, 20, 1], [60 + i * 80, 60, 1]],
  size: 8,
  color: { r: 255, g: 0, b: 0, a: 255 },
}));
const before = await call("list_atoms", { limit: 50 });
const tb = await call("try_batch", { layer_id: "p", strokes, label: LABEL });
console.log(`  try_batch ⇒ ok=${tb.ok}｜count=${tb.count}｜attempt_ids=${(tb.attempt_ids ?? []).length}`);
if (tb.ok !== true) failures.push(`try_batch 失败 ⇒ ${JSON.stringify(tb).slice(0, 120)}`);
if (Number(tb.count) !== 3 || (tb.attempt_ids ?? []).length !== 3) failures.push(`应返回 3 笔 3 个 attempt_ids，实测 ${tb.count}/${(tb.attempt_ids ?? []).length} ✗`);
if (!(Number(tb.pixels_touched_total) > 0)) failures.push(`pixels_touched_total=${tb.pixels_touched_total}，应大于 0 ✗`);

// ⑤ 顶层必须有 attempt_id / bbox / pixels_touched（外部 §117 的痛点：这些只放在 per_stroke 里，
//    他们按顶层取值 ⇒ 看到 None ⇒ 说"闭环断了"）
console.log(`  顶层字段 ⇒ attempt_id=${String(tb.attempt_id).slice(-6)}｜bbox=${JSON.stringify(tb.bbox)}｜pixels_touched=${tb.pixels_touched}`);
if (!tb.attempt_id) failures.push("顶层没有 attempt_id ⇒ 调用方无从调用 commit_batch ✗");
const topBbox = tb.bbox;
if (!Array.isArray(topBbox) || topBbox.length !== 4) failures.push(`顶层 bbox 应是 4 元组，实测 ${JSON.stringify(topBbox)} ✗`);
if (!(Number(tb.pixels_touched) > 0)) failures.push(`顶层 pixels_touched=${tb.pixels_touched}，应大于 0 ✗`);
// 恒等式：顶层像素数 ≥ 每一笔的像素数（合计不小于单笔）
for (const one of tb.per_stroke ?? []) {
  if (Number(tb.pixels_touched) < Number(one.pixels_touched)) {
    failures.push(`顶层 pixels_touched=${tb.pixels_touched} 小于单笔 ${one.pixels_touched} ⇒ 合计算错了 ✗`);
    break;
  }
}

const mid = await call("list_atoms", { limit: 50 });
console.log(`  试画后笔迹数 ${before.count} ⇒ ${mid.count}`);
if (Number(mid.count) !== Number(before.count)) failures.push(`try_batch 落笔了：笔迹数从 ${before.count} 变成 ${mid.count} ✗`);

const cb = await call("commit_batch", { attempt_ids: tb.attempt_ids });
console.log(`  commit_batch ⇒ ok=${cb.ok}｜atom_ids=${(cb.atom_ids ?? []).length}｜committed_as=${cb.committed_as}`);
if (cb.ok !== true) failures.push(`commit_batch 失败 ⇒ ${JSON.stringify(cb).slice(0, 120)}`);
if ((cb.atom_ids ?? []).length !== 3) failures.push(`应返回 3 个 atom_ids，实测 ${(cb.atom_ids ?? []).length} ✗`);
if (cb.committed_as !== "bitmap") failures.push(`committed_as 应写 bitmap，实测 ${JSON.stringify(cb.committed_as)} ✗`);

const after = await call("list_atoms", { limit: 50 });
console.log(`  提交后笔迹数 ${mid.count} ⇒ ${after.count}`);
if (Number(after.count) !== Number(mid.count) + 3) failures.push(`提交后笔迹数应从 ${mid.count} 变成 ${mid.count + 3}，实测 ${after.count} ✗`);

const byLabel = await call("list_atoms", { label: LABEL });
console.log(`  按 label=${LABEL} 查 ⇒ ${byLabel.count} 条`);
if (Number(byLabel.count) !== 3) failures.push(`整组标签应查到 3 条，实测 ${byLabel.count} ⇒ 标签没跟着走 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 批量：试画不落笔、提交贴 3 笔、返回 atom_ids、整组标签跟着走");
