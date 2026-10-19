#!/usr/bin/env node
// **`list_brushes(medium=X)` 必须真的筛选** —— 对 `mediums` 报出的每个名字（第 1048 轮）
//
// **∴ 缺陷（第 1047 轮查清）**：
//   `medium_stroke` 接受 6 个介质名（oil/watercolor/marker/pencil/pixel/example），
//   而 `medium_brush_keywords` 的 match **只有 5 个臂**（oil/watercolor/sketch/ink/pastel）
//   ⇒ 其余走 `_ => None` ⇒ `:5837` 的 `.and_then(...)` 透传 None
//   ⇒ `:5849` 的 `if let Some(keywords)` 不进入 ⇒ **不筛选 ⇒ 返回全部** ✗
//
// **∴ 实测**（全部 201 支）：
//   oil 28｜watercolor 25｜sketch 19｜ink 15｜pastel 10  ⇒ 真筛选 ✓
//   marker 201｜pencil 201｜pixel 201                     ⇒ 没筛选 ✗
//
// **∴ 判据**：`mediums + example_mediums` 里每个名字，`list_brushes(medium=X)` 的支数
//   必须**严格小于**「不带 medium」的支数（否则说明没筛）。
//
// **∴ 当前状态**：**预期红**（marker/pencil/pixel 三条）⇒ 已登记进 criteria-known-red.txt ✓
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-medium-filter-covers-all.mjs <base-url>"); process.exit(2); }
const doc = `medf-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "medium-filter-")));
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
const names = [...(caps?.mediums ?? []), ...(caps?.example_mediums ?? [])];
if (!names.length) { console.error("  ⊘ 不可判：拿不到 mediums（判据前提不成立）"); process.exit(3); }
const all = await call("list_brushes", {});
const total = (all.brushes ?? []).length;
if (total === 0) { console.error("  ⊘ 不可判：不带 medium 时返回 0 支（判据前提不成立）"); process.exit(3); }

const bad = [];
for (const m of names) {
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
console.log(`  ✓ mediums 的 ${names.length} 个名字都真的在筛选（都不等于全部 ${total} 支）`);
