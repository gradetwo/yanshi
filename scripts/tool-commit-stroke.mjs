#!/usr/bin/env node
// **`try_stroke` → `commit_stroke` 必须闭环**（需求 v2 §2.5 方案 B）
//   ① 提交后笔迹数 +1 ② 返回 atom_id 与 committed_as=bitmap
//   ③ 同一尝试不能提交两次 ④ 不存在的编号要报错并列出已有的
//   ⑤ 标签要跟着提交进原子（实测：设错字段就会静默丢）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-commit-stroke.mjs <base-url>"); process.exit(2); }
const doc = `cm-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "commit-stroke-")));
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
const LABEL = "COMMIT-LBL";
const failures = [];
const t = await call("try_stroke", {
  layer_id: "p", brush: "100%_Opaque", points: [[20, 20, 1], [80, 80, 1]], size: 12,
  color: { r: 255, g: 0, b: 0, a: 255 }, want_image: false, label: LABEL,
});
if (!t.attempt_id) { console.error("判据自身前提不成立：try_stroke 没返回 attempt_id（不是产品问题）"); process.exit(3); }
const before = await call("list_atoms", { limit: 50 });
const cm = await call("commit_stroke", { attempt_id: t.attempt_id });
const after = await call("list_atoms", { limit: 50 });
console.log(`  commit ⇒ ok=${cm.ok}｜committed_as=${cm.committed_as}｜笔迹数 ${before.count} ⇒ ${after.count}`);
if (cm.ok !== true) failures.push(`commit_stroke 失败 ⇒ ${JSON.stringify(cm).slice(0, 120)}`);
if (Number(after.count) !== Number(before.count) + 1) failures.push(`提交后笔迹数应从 ${before.count} 变成 ${before.count + 1}，实测 ${after.count} ✗`);
if (cm.committed_as !== "bitmap") failures.push(`committed_as 应写 bitmap（方案 B），实测 ${JSON.stringify(cm.committed_as)} ✗`);
if (!cm.atom_id) failures.push("commit_stroke 没返回 atom_id ⇒ 没法配合 revert ✗");

// 标签要跟着提交进原子
const byLabel = await call("list_atoms", { label: LABEL });
console.log(`  按 label 查 ⇒ ${(byLabel.atoms ?? []).length} 条`);
if ((byLabel.atoms ?? []).length !== 1) failures.push(`提交后按 label 应查到 1 条，实测 ${(byLabel.atoms ?? []).length} ⇒ 标签没跟着提交 ✗`);

// 同一尝试不能提交两次
const again = await call("commit_stroke", { attempt_id: t.attempt_id });
console.log(`  重复提交 ⇒ ok=${again.ok}`);
if (again.ok === true) failures.push("同一尝试提交两次竟然成功 ⇒ 会重复落笔 ✗");

// 不存在的编号要报错
const bad = await call("commit_stroke", { attempt_id: "nope-xyz" });
const detail = String((bad.context || {}).detail || "");
console.log(`  不存在的编号 ⇒ ok=${bad.ok}｜${detail.slice(0, 60)}`);
if (bad.ok !== false) failures.push("不存在的 attempt_id 竟然 ok=true ✗");
if (!/现有/.test(detail)) failures.push("错误信息里没列出已有的尝试 ⇒ 调用方没法自查 ✗");

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 试了能要：笔迹数 +1、committed_as=bitmap、标签跟着走、不可重复提交、错编号报错并列出已有的");
