#!/usr/bin/env node
// **`fill` / `draw_stroke` 等 write_draw 系列必须收 `label`**（外部 §113 清单第 2 项：
// 他们的 fill 出错同样难查，E815 查了两轮才定位）
//   ① fill 带 label ⇒ 能按 label 查到，且种类是 fill
//   ② draw_stroke 带 label ⇒ 同样能查到
//   ③ 不带 label ⇒ 查不到（不许把空标签也列出来）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-draw-label.mjs <base-url>"); process.exit(2); }
const doc = `dl-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "draw-label-")));
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
const failures = [];

// ① fill 带 label
const FILL = "FILL-LBL";
const f = await call("fill", {
  layer_id: "p", label: FILL,
  data: { points: [[20, 20], [180, 20], [180, 180], [20, 180]], color: { r: 255, g: 0, b: 0, a: 255 } },
});
console.log(`  fill(label) ⇒ ok=${f.ok}`);
if (f.ok !== true) failures.push(`fill 带 label 失败 ⇒ ${JSON.stringify(f).slice(0, 120)}`);
const byFill = await call("list_atoms", { label: FILL, include_non_stroke: true });
console.log(`  按 label 查 fill ⇒ ${byFill.count} 条｜种类=${[...new Set((byFill.atoms ?? []).map((a) => a.kind))].join(",")}`);
if (Number(byFill.count) !== 1) failures.push(`fill 带 label 应查到 1 条，实测 ${byFill.count} ⇒ label 没写进原子 ✗`);
if ((byFill.atoms ?? [])[0]?.kind !== "fill") failures.push(`查到的种类应是 fill，实测 ${JSON.stringify((byFill.atoms ?? [])[0]?.kind)} ✗`);

// ② draw_stroke 带 label
const DS = "DRAW-LBL";
const d = await call("draw_stroke", {
  layer_id: "p", label: DS,
  data: { points: [[20, 20, 1], [80, 80, 1]], brush: "100%_Opaque", size: 8, color: { r: 0, g: 0, b: 255, a: 255 } },
});
console.log(`  draw_stroke(label) ⇒ ok=${d.ok}`);
if (d.ok !== true) failures.push(`draw_stroke 带 label 失败 ⇒ ${JSON.stringify(d).slice(0, 120)}`);
const byDraw = await call("list_atoms", { label: DS, include_non_stroke: true });
console.log(`  按 label 查 draw_stroke ⇒ ${byDraw.count} 条`);
if (Number(byDraw.count) !== 1) failures.push(`draw_stroke 带 label 应查到 1 条，实测 ${byDraw.count} ✗`);

// ③ 不带 label ⇒ 查不到空标签
const none = await call("list_atoms", { label: "", include_non_stroke: true });
console.log(`  空标签查询 ⇒ ${none.count} 条`);
if (Number(none.count) !== 0) failures.push(`查空标签应得 0 条，实测 ${none.count} ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ fill 与 draw_stroke 都收 label，且能按它查回来");
