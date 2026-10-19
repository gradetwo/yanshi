#!/usr/bin/env node
// **四个 smart 工具必须真落笔**（2026-10-19 补丁；它们是"塑料感"的对策）
//   ① 四个都 ok=true ② 每个都真的增加了原子数（不是只解析参数）
//   ③ 各自的返回里带结构化字段（strokes / layers / edge_points …）
//   ④ failed 计数必须为 0（有失败要照实报）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-smart-tools.mjs <base-url>"); process.exit(2); }
const doc = `st-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "smart-tools-")));
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
const failures = [];
// 颜色是 **array** 不是对象（第一次我就是传对象错的）
const A = [0.86, 0.24, 0.16, 1.0];
const B = [0.16, 0.24, 0.86, 1.0];
const shape = { type: "rect", x: 40, y: 40, w: 140, h: 100 };

const cases = [
  ["smart_fill", { layer_id: "p", shape, brush: "2B_pencil", color: A }, "strokes"],
  ["smart_blend", { layer_id: "p", shape, color_a: A, color_b: B }, "pass1_lines"],
  ["smart_glaze", { layer_id: "p", shape, color: A }, "layers"],
  ["smart_edge_soften", { layer_id: "p", shape }, "edge_points"],
];

// **不能用 `after.count > before.count`**：`list_atoms` 的 limit 上限是 500，
// 当原子数超过 500 后两边都是 500 ⇒ 断言恒假（第 677 轮实测：smart_edge_soften 把它顶到 500）。
// 改用 `since_seq`：只数"这一笔新增的"，不受总量上限影响。
for (const [tool, args, key] of cases) {
  // **不传 order**：那是 list_suggestions 的参数，list_atoms 没有它（第 678 轮踩过）。
  // list_atoms 缺省就按 seq 倒序 ⇒ limit:1 就是最新的那条。
  const before = await call("list_atoms", { limit: 1 });
  const maxSeq = (before.atoms ?? []).reduce((m, a) => Math.max(m, Number(a.seq)), 0);
  const r = await call(tool, args);
  const added = await call("list_atoms", { limit: 500, since_seq: maxSeq });
  const grew = Number(added.count) > 0;
  const hasKey = key in r;
  console.log(`  ${tool} ⇒ ok=${r.ok}｜新增 ${added.count} 个原子（since_seq=${maxSeq}）｜有 ${key}=${hasKey}｜failed=${r.failed}`);
  if (r.ok !== true) failures.push(`${tool} 失败 ⇒ ${JSON.stringify(r.context || {}).detail} ✗`);
  if (!grew) failures.push(`${tool} 没新增原子（since_seq=${maxSeq} 之后 0 个）⇒ 只解析了参数没落笔 ✗`);
  if (!hasKey) failures.push(`${tool} 的返回里没有 ${key} ⇒ 形状信息缺失 ✗`);
  if (r.failed !== undefined && Number(r.failed) !== 0) failures.push(`${tool} 的 failed=${r.failed}，应 0 ✗`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 四个 smart 工具都真落笔、都带形状字段、failed 都是 0");
