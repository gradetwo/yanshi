#!/usr/bin/env node
// **D01 验收判据**（《山海同心》交付包 P2）：「笔触日志混入形状与填充」
//  ⇒ `list_atoms` 缺省（不给 `include_non_stroke`）必须**只返回笔迹原子**；
//     给了 `include_non_stroke: true` 才把形状/填充等一起列出。
//  这是「静默降级」的反面：调用方要能靠一个布尔控制看的范围。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-atoms-exclude-nonstroke.mjs <base-url>"); process.exit(2); }
const doc = `d01-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "atoms-nonstroke-")));
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
// 一笔笔迹
await call("brush_stroke", {
  layer_id: "p", brush: "100%_Opaque", size: 10,
  color: [0, 0, 0, 1], points: [[30, 30, 1], [120, 120, 1]],
});
// 一个形状 + 一块填充（**不该出现在缺省的笔迹日志里**）
await call("draw_shape", { layer_id: "p", shape: { type: "rect", x: 150, y: 30, w: 80, h: 80 }, stroke: [0, 0, 0, 1] });
await call("fill_region", { layer_id: "p", shape: { type: "rect", x: 30, y: 150, w: 80, h: 80 }, color: [1, 0, 0, 1] });

const plain = await call("list_atoms", { limit: 100 });
const withNon = await call("list_atoms", { limit: 100, include_non_stroke: true });
const kinds = (r) => [...new Set((r.atoms ?? []).map((a) => a.kind))].sort();
const kPlain = kinds(plain);
const kWith = kinds(withNon);
console.log(`  缺省 ⇒ ${plain.count} 条｜种类=${JSON.stringify(kPlain)}`);
console.log(`  含非笔迹 ⇒ ${withNon.count} 条｜种类=${JSON.stringify(kWith)}`);

const STROKE = new Set(["import_image"]);
const failures = [];
// ① 缺省里不许出现非笔迹种类
const leaked = kPlain.filter((k) => !STROKE.has(k));
if (leaked.length) failures.push(`缺省的笔迹日志里混进了 ${JSON.stringify(leaked)} ⇒ 正是 D01 那条缺陷 ✗`);
// ② 缺省的条数必须 < 含非笔迹的条数（否则那个开关没起作用）
if (!(Number(plain.count) < Number(withNon.count))) {
  failures.push(`打开 include_non_stroke 之后条数没变（${plain.count} vs ${withNon.count}）⇒ 开关没生效 ✗`);
}
// ③ 抬上来的种类里必须真的有形状/填充（否则测试数据没造出该情况）
if (kWith.length <= kPlain.length) failures.push(`含非笔迹时种类没增加 ⇒ 输入没造出要检查的情况（判据自身问题）`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 缺省的笔迹日志只有笔迹；打开开关才带上形状与填充");
