#!/usr/bin/env node
// **三种选区形状都必须能用来裁剪** ✓（第 1000+ 轮 ✓；YANSHI_ISSUES 的 §13「幽灵选区」）
//
// **∴ 修前实测 ✓**：`create_selection(rect)` ⇒ `ok=True` ✓、`list_selections` ⇒ `count=1` ✓（**∴ 看得见 ✓**）
//   而 `brush_stroke(clip_to_selection="s_rect")` ⇒ **失败** ✗
//   「找不到可裁剪的选区 s_rect ⇒ 现有：（一个都没有 ✓）」✗
//   ⇒ **∴ 同一个 id 在"存在"与"不存在"并存 ✓，且错误信息说"一个都没有"✗** —— **∴ 极具误导性 ✓**
//
// **∴ 根因 ✓**：`clip_to_selection` 把 `selections` 映射成 `(id, bbox)` 时，
//   `match shape.get("bbox")` 的 `None` 分支**只读 `shape.get("points")`** ✗（两处都用 `?`）
//   ⇒ **∴ `rect`／`ellipse` 既无 `bbox` 又无 `points` ⇒ **被 `filter_map` 静默丢弃**✓
//   ⇒ **∴ `alive` 为空 ⇒ 报"一个都没有"✗**
//
// **∴ 这条判据守什么 ✓**：三种形状各建一个选区 ⇒ **每个都要能 `clip`** ✓
//   ⇒ **∴ 变异**：去掉新加的 `rect`／`ellipse` 分支 ⇒ 那两个必红 ✓
import { readFileSync } from "node:fs";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-selection-shapes.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
const SHAPES = {
  rect: { kind: "rect", x: 60, y: 60, width: 80, height: 80 },
  ellipse: { kind: "ellipse", cx: 100, cy: 100, rx: 40, ry: 40 },
  polygon: { kind: "polygon", points: [[60, 60], [140, 60], [140, 140], [60, 140]] },
};
await call("create_layer", { layer_id: "sel_L", name: "sel_L" });

let bad = 0;
for (const [kind, shape] of Object.entries(SHAPES)) {
  const id = `sel_${kind}`;
  const created = await call("create_selection", { selection_id: id, shape });
  if (created.ok !== true) { console.log(`  ✗ ${kind}: create_selection 失败 ${JSON.stringify(created).slice(0, 120)}`); bad += 1; continue; }
  const listed = await call("list_selections", {});
  // 字段名是 selection_id（不是 id ✗）—— 我第一次写 s.id，于是三种形状全都显示"看不见" ✗，
  // 而 clip 其实是通的。判据自己错的时候，它给出的"看不见"会把人往错的方向带。
  const visible = (listed.selections ?? []).some((s) => s.selection_id === id);
  const clipped = await call("brush_stroke", {
    layer_id: "sel_L", brush: "100%_Opaque",
    color: { r: 10, g: 10, b: 10, a: 255 },
    points: [[20, 100, 1], [180, 100, 1]],
    clip_to_selection: id, preview: false,
  });
  const ok = clipped.ok === true;
  const detail = ok ? "" : `｜${String((clipped.context ?? {}).detail ?? "").slice(0, 90)}`;
  console.log(`  ${ok ? "✓" : "✗"} ${kind}：create ✓／list ${visible ? "看得见 ✓" : "**看不见** ✗"}／**clip ${ok ? "ok" : "失败"}**${detail}`);
  if (!ok) bad += 1;
}
if (bad) { console.log(`  ✗ ${bad} 种形状的选区不能用来裁剪（§13 幽灵选区）`); process.exit(1); }
console.log("  ✓ 三种形状（rect／ellipse／polygon）的选区都能用来裁剪");
