#!/usr/bin/env node
// **椭圆选区必须按**椭圆**裁，不能按外接矩形裁** ✓
//
// **∴ 为什么必须像素级 ✗**：`tool-selection-shapes.mjs` 只断言 `ok=true` ✗ ⇒ **∴ 它**测不出形状语义**✗**
//   ⇒ **∴ 我因此**猜过一个错结论**✗**（"ellipse 按外接矩形裁"✗）
//   ⇒ **∴ 而代码里其实有 `ellipse_coverage_clipped`**✓（`tools.rs:16821` ✓）
//
// **∴ 判据 ✓**：椭圆选区（cx=100,cy=100,rx=40,ry=40 ✓）＋ 一比横穿 ✓ ⇒ 采 5 点 ✓
//   · **中心 (100,100)** ⇒ **必须有墨**✓（∴ 在椭圆内 ✓）
//   · **四角 (62,62)／(138,62)／(62,138)／(138,138)** ⇒ **必须无墨**✗
//     （∴ 它们在外接矩形内 ✓、但在椭圆外 ✓ ⇒ **∴ 若按矩形裁 ⇒ 会有墨 ⇒ 判据红 ✓**）
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-ellipse-clip.mjs <base-url>"); process.exit(3); }

const boot = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc: `ellipse_clip_${Date.now()}`, width: 200, height: 200 }),
});
const bootBody = await boot.json().catch(() => ({}));
const token = bootBody.token;
if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(3); }
const doc = bootBody.doc ?? bootBody.doc_id ?? "ellipse_clip";

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) });
  return await reply.json();
};

await call("create_layer", { layer_id: "ec_L", name: "ec_L" });
// 铺白底，便于判断"有没有墨"
await call("fill_region", { layer_id: "ec_L", shape: { type: "rect", x: 0, y: 0, w: 200, h: 200 }, color: { r: 255, g: 255, b: 255, a: 255 } });
const sel = await call("create_selection", { selection_id: "ec_s", shape: { kind: "ellipse", cx: 100, cy: 100, rx: 40, ry: 40 } });
if (sel.ok !== true) { console.log(`  ✗ 建椭圆选区失败：${JSON.stringify(sel).slice(0, 140)}`); process.exit(1); }

const stroke = await call("brush_stroke", {
  layer_id: "ec_L", brush: "100%_Opaque",
  color: { r: 0, g: 0, b: 0, a: 255 },
  points: [[150, 100, 1], [50, 100, 1]],
  size: 60, preview: false, clip_to_selection: "ec_s",
});
if (stroke.ok !== true) { console.log(`  ✗ 裁剪笔触失败：${JSON.stringify(stroke).slice(0, 160)}`); process.exit(1); }

const samples = await call("sample_colors", { points: [[100, 100], [62, 62], [138, 62], [62, 138], [138, 138]] });
if (samples.ok !== true) { console.log("  ✗ sample_colors 失败"); process.exit(3); }
const rgba = (samples.samples ?? []).map((s) => s.rgba);
const isWhite = (c) => Array.isArray(c) && c[0] > 240 && c[1] > 240 && c[2] > 240;
const isInk = (c) => Array.isArray(c) && c[0] < 40 && c[1] < 40 && c[2] < 40;

let bad = 0;
const centerOk = isInk(rgba[0]);
console.log(`  ${centerOk ? "✓" : "✗"} 椭圆中心 (100,100) ⇒ ${JSON.stringify(rgba[0])}（**必须有墨** ✗）`);
if (!centerOk) bad += 1;

const CORNERS = [[62, 62], [138, 62], [62, 138], [138, 138]];
for (let i = 0; i < CORNERS.length; i += 1) {
  const c = rgba[i + 1];
  const ok = isWhite(c);
  console.log(`  ${ok ? "✓" : "✗"} 外接矩形内但椭圆外 (${CORNERS[i].join(",")}) ⇒ ${JSON.stringify(c)}（**必须无墨** ✗）`);
  if (!ok) bad += 1;
}

if (bad) {
  console.log(`  ✗ ${bad} 处不符 ⇒ **椭圆选区是按外接矩形裁的**（不是椭圆）✗`);
  process.exit(1);
}
console.log("  ✓ 椭圆选区真的按椭圆裁：中心有墨、四个角无墨");
