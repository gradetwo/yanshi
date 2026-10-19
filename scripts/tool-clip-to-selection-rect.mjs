#!/usr/bin/env node
// **`brush_stroke(clip_to_selection=…)` 必须能用 `rect` 选区裁剪**（第 901 轮；真 bug）
//
//  根因：`create_selection` 存的 shape 是 `{kind:"rect", x, y, w, h}`
//        而 `tools.rs` 的 clip 解析读的是 `num("width")` / `num("height")`
//        ⇒ **永远 None** ⇒ 被 `filter_map` 丢掉 ⇒ 报「找不到可裁剪的选区 ⇒ 现有：（一个都没有）」
//  ⇒ 这条路径**从来没连通**；判据锁住「存什么字段就读什么字段」。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-clip-to-selection-rect.mjs <base-url>"); process.exit(2); }
const doc = `clip-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "clip-rect-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 100, height: 100 }),
})).json();
if (!created.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${created.token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "a", name: "a" });
const sel = await call("create_selection", {
  selection_id: "s", shape: { kind: "rect", x: 10, y: 10, w: 50, h: 50 },
});
if (sel.ok !== true) { console.error("  ⊘ 不可判：建不了 rect 选区（判据前提不成立）"); process.exit(3); }

const r = await call("brush_stroke", {
  layer_id: "a", brush: "100%_Opaque.myb", size: 30,
  points: [[20, 20], [50, 50]], clip_to_selection: "s", color: [1, 0, 0, 1],
});
const failures = [];
if (r.ok !== true) {
  failures.push(`brush_stroke(clip_to_selection="s") 被拒 ⇒ ${JSON.stringify(r.context ?? {}).slice(0, 120)}`);
}
const px = await call("sample_colors", { points: [[30, 30], [85, 85]] });
const [inside, outside] = (px.samples ?? []).map((s) => (s.rgba ?? []).slice(0, 3).join(","));
console.log(`  ok=${r.ok}｜选区内(30,30)=${inside}｜选区外(85,85)=${outside}`);
if (inside !== "255,0,0") failures.push(`选区内没落墨（${inside}）⇒ clip 没生效`);
if (outside === "255,0,0") failures.push(`选区外也落墨了（${outside}）⇒ clip 没裁住`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ rect 选区能被 clip_to_selection 正确裁剪（内红、外白）");
