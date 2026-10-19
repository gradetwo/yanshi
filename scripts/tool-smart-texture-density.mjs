#!/usr/bin/env node
// **`smart_texture` 的 `density` 必须单调递增**（第 750 轮）
//  实测（小形状 100×100，远离补丁说的上限 240）：
//    density 0.1 → planned 10｜0.3 → 21｜0.5 → 28｜0.8 → 37｜1.0 → 41
//  ⇒ 严格递增。若哪天不递增，说明密度映射坏了（而它仍会报 ok=true ⇒ 静默降级）。
//  **必须用小形状**：360×360 上三档会撞上限 240（实测 210/199/199），看不出单调性。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-smart-texture-density.mjs <base-url>"); process.exit(2); }
const doc = `st-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "texture-density-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "p", name: "p" });
const densities = [0.1, 0.3, 0.5, 0.8, 1.0];
const planned = [];
for (const d of densities) {
  const r = await call("smart_texture", {
    layer_id: "p", shape: { kind: "rect", x: 20, y: 20, w: 100, h: 100 },
    texture_type: "stone", density: d,
  });
  if (r.ok !== true) { console.error(`density=${d} 报错：${JSON.stringify(r.context ?? {})}`); process.exit(1); }
  planned.push(Number(r.planned));
  // 恒等式：真正画出的条数不许超过计划条数
  if (Number(r.strokes) > Number(r.planned)) {
    console.error(`density=${d}：strokes ${r.strokes} > planned ${r.planned} ⇒ 计划与实际矛盾 ✗`);
    process.exit(1);
  }
}
console.log(`  density ${densities.join("/")} ⇒ planned ${planned.join("/")}`);

const failures = [];
// ① 严格递增
for (let i = 1; i < planned.length; i += 1) {
  if (!(planned[i] > planned[i - 1])) {
    failures.push(`density ${densities[i - 1]}→${densities[i]} 的 planned 没增加（${planned[i - 1]}→${planned[i]}）⇒ 密度映射坏了 ✗`);
  }
}
// ② 全部 > 0（否则「画了 0 条」而报 ok=true 就是静默降级）
if (planned.some((v) => !(v > 0))) failures.push(`有 density 档位的 planned 是 0 ⇒ 报 ok 却没计划任何笔 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ density 严格单调递增，且 strokes ≤ planned");
