#!/usr/bin/env node
// **D04 验收判据**（《山海同心》交付包 P1）：非法 `anchor_mode` 必须**报错且一个原子都不写**。
//  那条缺陷是「报错后仍写入笔触」⇒ 静默降级的一种。修完后两条都要成立：
//   ① 返回 ok=false ② 原子数**不变**（不能"报了错但还是画了"）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-anchor-mode-rejects.mjs <base-url>"); process.exit(2); }
const doc = `am-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "anchor-mode-")));
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
const before = await call("list_atoms", { limit: 1 });
const seqBefore = (before.atoms ?? []).reduce((m, a) => Math.max(m, Number(a.seq)), 0);

// 一个**明确非法**的 anchor_mode（合法值只有 data）
const r = await call("brush_stroke", {
  layer_id: "p", brush: "100%_Opaque", size: 10,
  color: [0, 0, 0, 1], points: [[20, 20, 1], [80, 80, 1]],
  anchor_mode: "definitely-not-a-mode",
});
// **只数笔迹原子**（缺省 `include_non_stroke: false`）：带上非笔迹原子会把测试自己
// 之前产生的 `create_document` / `create_layer` 也算进来，看起来像"报错后还是画了"（第 605 轮踩过）。
const added = await call("list_atoms", { limit: 500, since_seq: seqBefore });
console.log(`  非法 anchor_mode ⇒ ok=${r.ok}｜新增原子=${added.count}`);
console.log(`    错误信息：${(r.context || {}).detail ?? (r.error ?? "(无)")}`);

const failures = [];
if (r.ok !== false) failures.push(`非法 anchor_mode 竟然返回 ok=true ⇒ 没校验 ✗`);
if (Number(added.count) !== 0) {
  failures.push(`报错之后仍写入 ${added.count} 个**笔迹**原子 ⇒ 「报了错还是画了」（正是 D04 那条缺陷）✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 非法 anchor_mode 被拒，且一个原子都没写");
