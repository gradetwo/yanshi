#!/usr/bin/env node
// **`paint_mask` 的核心承诺：物理隔离，擦一个蒙版不碰别的图层**（第 741 轮）
//  之前我写错过两次：① 以为 paint_mask 擦画面（其实它只改蒙版）；
//  ② 以为蒙版外应当保留颜色（其实蒙版让图层**只在蒙版内可见**）。
//  正确的最小测试：两个图层各铺色，只给 A 挂蒙版，用 paint_mask 改 A 的蒙版
//  ⇒ **B 的像素一个都不许变**。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-paint-mask-isolation.mjs <base-url>"); process.exit(2); }
const doc = `pm-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "paint-mask-")));
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
const px = async (x, y) => {
  const r = await call("sample_colors", { points: [[x, y]] });
  const s = (r.samples ?? [])[0];
  return s && s.rgba ? s.rgba.slice(0, 3) : null;
};
const same = (a, b) => a && b && a.every((v, i) => Math.abs(v - b[i]) <= 2);

// A 层：红；B 层：蓝（B 在下）
await call("create_layer", { layer_id: "b", name: "b" });
await call("fill_region", { layer_id: "b", shape: { kind: "rect", x: 0, y: 0, w: 300, h: 300 }, color: [0.1, 0.1, 0.9, 1.0] });
await call("create_layer", { layer_id: "a", name: "a" });
await call("fill_region", { layer_id: "a", shape: { kind: "rect", x: 0, y: 0, w: 300, h: 300 }, color: [0.9, 0.1, 0.1, 1.0] });
// 蒙版只盖 A 的右半，挂到 A
await call("create_mask", { mask_id: "m1", shape: { kind: "rect", x: 150, y: 0, w: 150, h: 300 } });
await call("set_property", { layer_id: "a", key: "mask_id", value: "m1" });

const leftOfA = await px(60, 150);   // A 的蒙版外 ⇒ 应看到 B 的蓝
const inA = await px(240, 150);      // A 的蒙版内 ⇒ 应看到 A 的红
const bProbe = await px(60, 150);    // B 的可见像素（A 蒙版外露出来的）

const r = await call("paint_mask", { mask_id: "m1", mode: "erase", size: 60, points: [[240, 150, 1], [260, 150, 1]] });
const leftAfter = await px(60, 150);
const inAAfter = await px(240, 150);
console.log(`  挂蒙版后：蒙版外=${JSON.stringify(leftOfA)}（应为 B 的蓝）｜蒙版内=${JSON.stringify(inA)}（应为 A 的红）`);
console.log(`  改 A 的蒙版（erase）⇒ ok=${r.ok}`);
console.log(`  蒙版外 ${JSON.stringify(leftOfA)} ⇒ ${JSON.stringify(leftAfter)}｜未变=${same(leftOfA, leftAfter)}`);
console.log(`  蒙版内 ${JSON.stringify(inA)} ⇒ ${JSON.stringify(inAAfter)}｜变了=${!same(inA, inAAfter)}`);

const failures = [];
if (!same(leftOfA, leftAfter)) failures.push("改 A 的蒙版却让**蒙版外**的像素变了 ⇒ 越界 ✗");
if (same(inA, inAAfter)) failures.push("擦掉的蒙版内区域没有露出下层 ⇒ 擦除没生效 ✗");
if (!same(bProbe, leftOfA)) failures.push("蒙版外没露出下层的蓝 ⇒ 蒙版的可见性语义不对（判据自身问题）");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 擦 A 的蒙版只影响 A 的蒙版内，蒙版外与下层都不动");
