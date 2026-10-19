#!/usr/bin/env node
// **`revert_atoms` 必须逐个撤销且照实报成败**（需求 v2 §4.4）
//   ① 单个 id ⇒ reverted_count=1、failed_count=0
//   ② 无效 id ⇒ ok=false 且 failed 里带 error（不假称成功）
//   ③ 空数组 ⇒ 报错
//   ④ 混合（1 个有效 + 1 个无效）⇒ reverted=1、failed=1 —— 不许因为一条坏就整批失败
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-revert-atoms.mjs <base-url>"); process.exit(2); }
const doc = `rv-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "revert-atoms-")));
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
const ids = [];
for (let i = 0; i < 3; i += 1) {
  const r = await call("brush_stroke", {
    layer_id: "p", brush: "100%_Opaque", points: [[20 + i * 10, 20, 1], [60 + i * 10, 60, 1]],
    size: 8, color: { r: 255, g: 0, b: 0, a: 255 }, label: `RV${i}`,
  });
  if (!r.atom_id) { console.error("判据自身前提不成立：brush_stroke 没返回 atom_id（不是产品问题）"); process.exit(3); }
  ids.push(r.atom_id);
}

// ① 单个
const one = await call("revert_atoms", { atom_ids: [ids[1]] });
console.log(`  单个 ⇒ ok=${one.ok}｜reverted=${one.reverted_count}｜failed=${one.failed_count}`);
if (Number(one.reverted_count) !== 1 || Number(one.failed_count) !== 0) failures.push(`单个撤销应 reverted=1/failed=0，实测 ${one.reverted_count}/${one.failed_count} ✗`);

// ⑤ 撤销后该原子必须从 list_atoms 消失（否则调用方会看到"撤销了却还在"）
const listedBefore = await call("list_atoms", { limit: 50 });
await call("revert_atoms", { atom_ids: [ids[0]] });
const listedAfter = await call("list_atoms", { limit: 50 });
const stillThere = (listedAfter.atoms ?? []).some((a) => a.atom_id === ids[0]);
console.log(`  列表 ${listedBefore.count} ⇒ ${listedAfter.count}｜被撤销那笔还在=${stillThere}`);
if (stillThere) failures.push("撤销后那个原子仍在 list_atoms 里 ⇒ 调用方会以为没撤销成功 ✗");
if (Number(listedAfter.count) !== Number(listedBefore.count) - 1) {
  failures.push(`撤销一笔后列表应从 ${listedBefore.count} 变成 ${listedBefore.count - 1}，实测 ${listedAfter.count} ✗`);
}

// ② 无效 id
const bad = await call("revert_atoms", { atom_ids: ["nope-xyz"] });
const badFailed = bad.failed ?? [];
console.log(`  无效 id ⇒ ok=${bad.ok}｜failed=${bad.failed_count}`);
if (bad.ok !== false) failures.push("无效 id 竟然 ok=true ✗");
if (badFailed.length !== 1 || !badFailed[0].error) failures.push("failed 项里没有 error ⇒ 调用方看不出为什么失败 ✗");

// ③ 空数组
const empty = await call("revert_atoms", { atom_ids: [] });
console.log(`  空数组 ⇒ ok=${empty.ok}`);
if (empty.ok !== false) failures.push("空数组竟然 ok=true ✗");

// ④ 混合：一条好一条坏
const mixed = await call("revert_atoms", { atom_ids: [ids[2], "nope-abc"] });
console.log(`  混合 ⇒ ok=${mixed.ok}｜reverted=${mixed.reverted_count}｜failed=${mixed.failed_count}`);
if (Number(mixed.reverted_count) !== 1 || Number(mixed.failed_count) !== 1) failures.push(`混合应 reverted=1/failed=1，实测 ${mixed.reverted_count}/${mixed.failed_count} ⇒ 一条坏就整批失败 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 逐个撤销、照实报成败、坏一条不拖累其余、空数组报错");
