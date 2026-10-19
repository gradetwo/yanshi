#!/usr/bin/env node
// **`group_id` 的四层判据**（用户 2026-10-19 裁定要做）
//   ① 组内原子确实共享该 id ② `list_atoms(group_id=…)` 只返回该组
//   ③ `revert_group` 只挖该组、不误伤别的组 ④ 再撤一次是幂等（0 条）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-group-id.mjs <base-url>"); process.exit(2); }
const doc = `gi-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "group-id-")));
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
const brush = { layer_id: "p", brush: "100%_Opaque", size: 8, color: { r: 255, g: 0, b: 0, a: 255 } };
for (let i = 0; i < 3; i += 1) {
  await call("brush_stroke", { ...brush, points: [[20 + i * 20, 20, 1], [60 + i * 20, 60, 1]], group_id: "GA" });
}
await call("brush_stroke", { ...brush, points: [[20, 120, 1], [60, 160, 1]], group_id: "GB" });

const failures = [];
const all = await call("list_atoms", { limit: 50 });
const ga = await call("list_atoms", { limit: 50, group_id: "GA" });
const gb = await call("list_atoms", { limit: 50, group_id: "GB" });
const none = await call("list_atoms", { limit: 50, group_id: "NOPE-组" });
console.log(`  全部=${all.count}｜GA=${ga.count}｜GB=${gb.count}｜NOPE=${none.count}`);
if (Number(ga.count) !== 3) failures.push(`GA 应有 3 条，实测 ${ga.count} ✗`);
if (Number(gb.count) !== 1) failures.push(`GB 应有 1 条，实测 ${gb.count} ✗`);
if (Number(none.count) !== 0) failures.push(`不存在的组应得 0 条，实测 ${none.count} ✗`);
// ② 组内原子的 group_id 必须都等于该组（不是靠别的条件凑出来的数）
if ((ga.atoms ?? []).some((a) => a.group_id !== "GA")) failures.push("GA 的结果里混进了别的组 ✗");
// ③ 恒等式：两组之和 ≤ 全部（筛选不能凭空多出来）
if (Number(ga.count) + Number(gb.count) > Number(all.count)) {
  failures.push(`GA+GB=${Number(ga.count) + Number(gb.count)} 比全部 ${all.count} 还多 ⇒ 筛选没生效 ✗`);
}

const r = await call("revert_group", { group_id: "GA" });
console.log(`  revert_group(GA) ⇒ ok=${r.ok}｜reverted=${r.reverted_count}`);
if (Number(r.reverted_count) !== 3) failures.push(`revert_group 应撤 3 条，实测 ${r.reverted_count} ✗`);
const after = await call("list_atoms", { limit: 50 });
const gb2 = await call("list_atoms", { limit: 50, group_id: "GB" });
console.log(`  撤销后：全部=${after.count}｜GB 仍在=${gb2.count}`);
if (Number(gb2.count) !== 1) failures.push(`撤 GA 把 GB 也撤了 ⇒ 误伤别的组 ✗`);
if (Number(after.count) !== 1) failures.push(`撤销后应剩 1 条，实测 ${after.count} ✗`);

const again = await call("revert_group", { group_id: "GA" });
console.log(`  再撤一次 ⇒ reverted=${again.reverted_count}（幂等，应 0）`);
if (Number(again.reverted_count) !== 0) failures.push(`重复撤销不幂等：又撤了 ${again.reverted_count} 条 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 组标识写入、筛选、整组撤销、幂等 四层都对");
