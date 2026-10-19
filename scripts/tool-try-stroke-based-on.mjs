#!/usr/bin/env node
// **`based_on` 必须继承上次参数、只覆盖本次传入的**（需求 v2 §2.1）
//   ① 只改 size ⇒ 像素数必须变（说明 size 生效）
//   ② 什么都不改 ⇒ 像素数必须与上次完全相同（说明其余都继承到了）
//   ③ 改 points ⇒ bbox 必须移动（说明 points 生效）
//   ④ 不存在的 based_on ⇒ 报错并列出已有的（不许静默忽略）
//   ⑤ 既无 layer_id 又无 based_on ⇒ 报错（必填项没被放宽到"永远可选"）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-try-stroke-based-on.mjs <base-url>"); process.exit(2); }
const doc = `bo-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "based-on-")));
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
const a = await call("try_stroke", {
  layer_id: "p", brush: "100%_Opaque", points: [[20, 20, 1], [80, 80, 1]], size: 12,
  color: { r: 255, g: 0, b: 0, a: 255 },
});
if (!a.ok || !a.attempt_id) { console.error("判据自身前提不成立：第一次 try_stroke 失败（不是产品问题）"); process.exit(3); }

const same = await call("try_stroke", { based_on: a.attempt_id });
console.log(`  第一次 ${a.pixels_touched} 像素｜based_on 不改 ${same.pixels_touched} 像素`);
if (same.ok !== true) failures.push(`based_on 不改竟然失败 ⇒ ${(same.context || {}).detail} ✗`);
if (Number(same.pixels_touched) !== Number(a.pixels_touched)) {
  failures.push(`什么都不改时像素数应从 ${a.pixels_touched} 保持不变，实测 ${same.pixels_touched} ⇒ 继承不全 ✗`);
}

const smaller = await call("try_stroke", { based_on: a.attempt_id, size: 6 });
console.log(`  based_on + size=6 ⇒ ${smaller.pixels_touched} 像素`);
if (smaller.ok !== true) failures.push(`based_on + size 失败 ⇒ ${(smaller.context || {}).detail} ✗`);
if (Number(smaller.pixels_touched) >= Number(a.pixels_touched)) {
  failures.push(`size 12→6 后像素数应减少，实测 ${a.pixels_touched}→${smaller.pixels_touched} ⇒ size 没生效 ✗`);
}

const moved = await call("try_stroke", { based_on: a.attempt_id, points: [[20, 120, 1], [80, 180, 1]] });
console.log(`  based_on + 改 points ⇒ bbox=${JSON.stringify(moved.bbox)}`);
if (moved.ok !== true) failures.push(`based_on + points 失败 ⇒ ${(moved.context || {}).detail} ✗`);
const movedY = Array.isArray(moved.bbox) ? Number(moved.bbox[1]) : -1;
const baseY = Array.isArray(a.bbox) ? Number(a.bbox[1]) : -1;
if (!(movedY > baseY)) failures.push(`points 下移到 y=120 起，bbox.y 应大于 ${baseY}，实测 ${movedY} ✗`);

const bad = await call("try_stroke", { based_on: "nope-xyz" });
console.log(`  不存在的 based_on ⇒ ok=${bad.ok}`);
if (bad.ok !== false) failures.push("不存在的 based_on 竟然 ok=true ⇒ 静默忽略 ✗");

const missing = await call("try_stroke", { brush: "100%_Opaque", points: [[1, 2, 1], [3, 4, 1]] });
console.log(`  既无 layer_id 又无 based_on ⇒ ok=${missing.ok}`);
if (missing.ok !== false) failures.push("缺 layer_id 且无 based_on 竟然 ok=true ⇒ 必填项被放宽坏了 ✗");

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ based_on 继承全部参数、只覆盖传入的、错编号报错、必填项仍必填");
