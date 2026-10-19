#!/usr/bin/env node
// **`smart_select` 在纯色块上必须精确选中整块**（第 769 轮）
//  实测：左半 150×300 纯红、右半纯蓝 ⇒ 从左边取种子得 `selected_area_px = 45000`（= 150×300）、
//  `bbox_fill_ratio = 1`、`diagnostic_warning = null`；从右边取种子同样得 45000。
//  ⇒ 这两条断言抓的是补丁说明里那个真 bug：泛洪垂直传播预标记游程导致的「梳子状」传播
//    （会让选中面积**偏小**、bbox 填充率**小于 1**）。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-smart-select-block.mjs <base-url>"); process.exit(2); }
const doc = `ms-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "smart-select-")));
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
await call("fill_region", { layer_id: "p", shape: { kind: "rect", x: 0, y: 0, w: 150, h: 300 }, color: [0.9, 0.1, 0.1, 1] });
await call("fill_region", { layer_id: "p", shape: { kind: "rect", x: 150, y: 0, w: 150, h: 300 }, color: [0.1, 0.1, 0.9, 1] });

const EXPECT = 150 * 300;   // 45000：半块的确切像素数
const failures = [];
for (const [tag, seed] of [["左半", [50, 150]], ["右半", [250, 150]]]) {
  const r = await call("smart_select", { seed, tolerance: 0.2, layer_id: "p" });
  const area = Number(r.selected_area_px);
  const fill = Number(r.bbox_fill_ratio);
  console.log(`  ${tag} seed=${JSON.stringify(seed)} ⇒ ok=${r.ok}｜area=${area}｜fill=${fill}｜warn=${JSON.stringify(r.diagnostic_warning)}`);
  if (r.ok !== true) { failures.push(`${tag} 报错 ⇒ ${JSON.stringify(r.context ?? {})} ✗`); continue; }
  // ① 面积必须**精确等于**半块像素数（梳子状传播会让它偏小）
  if (area !== EXPECT) failures.push(`${tag} 的 area=${area}，应精确等于 ${EXPECT}（半块）⇒ 泛洪漏像素 ✗`);
  // ② bbox 填充率必须 = 1（纯色矩形块的 bbox 就是块本身）
  if (Math.abs(fill - 1) > 1e-6) failures.push(`${tag} 的 bbox_fill_ratio=${fill}，纯色块应为 1 ⇒ 选中区域不连续 ✗`);
  // ③ 纯色块不该报泄漏/噪点告警
  if (r.diagnostic_warning) failures.push(`${tag} 报了未经预期的告警 ${JSON.stringify(r.diagnostic_warning)} ✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 纯色块被精确选中（area 与 bbox 填充率都对）");
