#!/usr/bin/env node
// **`list_engine_capabilities.mediums` 的每个名字都必须真的能筛笔刷**（第 1005 轮）
//
// **∴ 来历**：旧值 ["oil","watercolor","marker","pencil","pixel"] 里有 3 个**无效名** ✗
//   ⇒ 实测 `list_brushes(medium="marker")` 返回 **201 支**（= 全部）⇒ **没筛选** ✗
//   ⇒ 而 oil/watercolor 各返回 28/25 ⇒ **真筛选** ✓
//   ⇒ 所以 Agent 用 marker/pencil/pixel 会**静默拿到全部**（以为筛过，其实没筛）✗
//   ⇒ 且旧值**漏了** sketch/ink/pastel 三个有效名 ✗
//
// **∴ 判据**：对 `mediums` 里每个名字，`list_brushes(medium=X)` 的支数必须
//   **严格小于**「不带 medium」的支数（否则说明没筛 ⇒ 该名字无效）。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-mediums-are-effective.mjs <base-url>"); process.exit(2); }
const doc = `med-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "mediums-effective-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 64, height: 64 }),
})).json();
if (!created.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${created.token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify(args),
}).then((r) => r.json());

const caps = await call("list_engine_capabilities", {});
const mediums = caps?.mediums ?? [];
if (!Array.isArray(mediums) || mediums.length === 0) {
  console.error("  ⊘ 不可判：list_engine_capabilities 没给 mediums（判据前提不成立）"); process.exit(3);
}
const all = await call("list_brushes", {});
const total = (all.brushes ?? []).length;
if (total === 0) {
  console.error("  ⊘ 不可判：不带 medium 时也返回 0 支（判据前提不成立）"); process.exit(3);
}
const bad = [];
for (const m of mediums) {
  const r = await call("list_brushes", { medium: m });
  const n = (r.brushes ?? []).length;
  const effective = r.ok === true && n > 0 && n < total;
  console.log(`  ${m.padEnd(12)} ok=${r.ok}｜笔刷=${n}/${total}${effective ? "" : "  ← 没筛选！"}`);
  if (!effective) bad.push(`${m}: ${n}/${total}`);
}
if (bad.length) {
  console.log(`  ✗ mediums 里有 ${bad.length} 个名字**没在筛选**（应为严格小于 ${total}）：${bad.join("；")}`);
  process.exit(1);
}
console.log(`  ✓ mediums 的 ${mediums.length} 个名字都真的在筛选（都不等于全部 ${total} 支）`);
