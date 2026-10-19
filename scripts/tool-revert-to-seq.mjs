#!/usr/bin/env node
// **`revert_to_seq` 必须回到「第 n 笔之后」**（需求 v2 §4.4）
//   ① 撤销后最大 seq ≤ n ② 条数按预期减少 ③ 幂等（再调一次不改动）④ 缺 seq 报错
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-revert-to-seq.mjs <base-url>"); process.exit(2); }
const doc = `rs-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "revert-to-seq-")));
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
for (let i = 0; i < 4; i += 1) {
  await call("brush_stroke", {
    layer_id: "p", brush: "100%_Opaque", points: [[20 + i * 10, 20, 1], [60 + i * 10, 60, 1]],
    size: 8, color: { r: 255, g: 0, b: 0, a: 255 }, label: `S${i}`,
  });
}
const before = await call("list_atoms", { limit: 50 });
const seqs = (before.atoms ?? []).map((a) => Number(a.seq)).sort((a, b) => a - b);
if (seqs.length < 4) { console.error("判据自身前提不成立：没有画够 4 笔（不是产品问题）"); process.exit(3); }
const cut = seqs[1];

const r = await call("revert_to_seq", { seq: cut });
console.log(`  画 4 笔（seq ${seqs.join(",")}）⇒ revert_to_seq(${cut}) ⇒ reverted=${r.reverted_count}`);
if (r.ok !== true) failures.push(`revert_to_seq 失败 ⇒ ${JSON.stringify(r).slice(0, 120)}`);
if (Number(r.reverted_count) !== 2) failures.push(`应撤销 2 笔（seq 大于 ${cut} 的那两笔），实测 ${r.reverted_count} ✗`);

const after = await call("list_atoms", { limit: 50 });
const maxSeq = Math.max(0, ...(after.atoms ?? []).map((a) => Number(a.seq)));
console.log(`  撤销后 ⇒ ${after.count} 条｜最大 seq=${maxSeq}`);
if (maxSeq > cut) failures.push(`撤销后最大 seq 是 ${maxSeq}，应 ≤ ${cut} ✗`);
if (Number(after.count) !== Number(before.count) - 2) failures.push(`条数应从 ${before.count} 变成 ${before.count - 2}，实测 ${after.count} ✗`);

const again = await call("revert_to_seq", { seq: cut });
console.log(`  再调一次 ⇒ reverted=${again.reverted_count}`);
if (Number(again.reverted_count) !== 0) failures.push(`同一序号再调应撤销 0 笔（幂等），实测 ${again.reverted_count} ✗`);

const bad = await call("revert_to_seq", {});
console.log(`  缺 seq ⇒ ok=${bad.ok}`);
if (bad.ok === true) failures.push("缺 seq 竟然 ok=true ✗");

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 回到第 n 笔之后、条数对、幂等、缺参数报错");
