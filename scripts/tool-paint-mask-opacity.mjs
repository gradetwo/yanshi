#!/usr/bin/env node
// **`paint_mask` 的 `opacity` 必须单调影响蒙版覆盖**（第 752 轮）
//  **必须先 erase 到 0 再 paint** —— `create_mask` 建出来的区域本来就是 1.0，
//  直接 paint 到 1.0 三档看不出差别（我第 751 轮就是这么误判成「opacity 可疑」的）。
//  实测（擦到 0 后，同一位置）：
//    opacity 0.2 → 211,89,159｜0.5 → 240,89,99｜1.0 → 243,89,89   ⇒ R 单调递增
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-paint-mask-opacity.mjs <base-url>"); process.exit(2); }
const doc = `po-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "paint-opacity-")));
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
const red = async (layer) => {
  const r = await call("sample_colors", { points: [[150, 150]] });
  const s = (r.samples ?? [])[0];
  return s && s.rgba ? s.rgba[0] : null;   // R 通道：底色蓝 R≈89，上层红 R≈243
};

const results = [];
for (const op of [0.2, 0.5, 1.0]) {
  const lid = `L${String(op).replace(".", "_")}`;
  await call("create_layer", { layer_id: `b${lid}`, name: `b${lid}` });
  await call("fill_region", { layer_id: `b${lid}`, shape: { kind: "rect", x: 0, y: 0, w: 300, h: 300 }, color: [0.1, 0.1, 0.9, 1.0] });
  await call("create_layer", { layer_id: lid, name: lid });
  await call("fill_region", { layer_id: lid, shape: { kind: "rect", x: 0, y: 0, w: 300, h: 300 }, color: [0.9, 0.1, 0.1, 1.0] });
  await call("create_mask", { mask_id: `m${lid}`, shape: { kind: "rect", x: 100, y: 0, w: 100, h: 300 } });
  // 关键：先擦到 0，否则 paint 到 1.0 看不出 opacity 的差别
  await call("paint_mask", { mask_id: `m${lid}`, mode: "erase", size: 120, points: [[150, -20, 1], [150, 320, 1]] });
  await call("set_property", { layer_id: lid, key: "mask_id", value: `m${lid}` });
  const r = await call("paint_mask", { mask_id: `m${lid}`, mode: "paint", opacity: op, size: 60, points: [[150, 20, 1], [150, 280, 1]] });
  if (r.ok !== true) { console.error(`opacity=${op} 报错：${JSON.stringify(r.context ?? {})}`); process.exit(1); }
  results.push({ op, r: await red(lid) });
}
console.log(`  opacity ${results.map((x) => x.op).join("/")} ⇒ R ${results.map((x) => x.r).join("/")}`);

const failures = [];
for (let i = 1; i < results.length; i += 1) {
  if (!(results[i].r > results[i - 1].r)) {
    failures.push(`opacity ${results[i - 1].op}→${results[i].op} 的 R 没增加（${results[i - 1].r}→${results[i].r}）⇒ opacity 静默失效 ✗`);
  }
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ opacity 单调影响覆盖（擦到 0 后重画）");
