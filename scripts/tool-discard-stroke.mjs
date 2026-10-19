#!/usr/bin/env node
// **`discard_stroke` 必须"丢掉但不落笔"**（需求 v2 §5.1 状态流转 pending → discarded）
//   ① 丢弃后笔迹数不变 ② 状态是 discarded ③ 之后 commit 必须被拒 ④ 重复丢弃必须被拒
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-discard-stroke.mjs <base-url>"); process.exit(2); }
const doc = `dc-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "discard-stroke-")));
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
const before = await call("list_atoms", { limit: 50 });
const t = await call("try_stroke", {
  layer_id: "p", brush: "100%_Opaque", points: [[20, 20, 1], [80, 80, 1]], size: 12,
  color: { r: 255, g: 0, b: 0, a: 255 }, want_image: false, label: "DISCARD-LBL",
});
if (!t.attempt_id) { console.error("判据自身前提不成立：try_stroke 没返回 attempt_id（不是产品问题）"); process.exit(3); }

const d = await call("discard_stroke", { attempt_id: t.attempt_id });
console.log(`  discard ⇒ ok=${d.ok}｜status=${d.status}`);
if (d.ok !== true) failures.push(`discard_stroke 失败 ⇒ ${JSON.stringify(d).slice(0, 120)}`);
if (d.status !== "discarded") failures.push(`状态应为 discarded，实测 ${JSON.stringify(d.status)} ✗`);

const after = await call("list_atoms", { limit: 50 });
console.log(`  笔迹数 ${before.count} ⇒ ${after.count}`);
if (Number(before.count) !== Number(after.count)) failures.push(`丢弃后笔迹数从 ${before.count} 变成 ${after.count} ⇒ 它落笔了 ✗`);

const cm = await call("commit_stroke", { attempt_id: t.attempt_id });
console.log(`  丢弃后再提交 ⇒ ok=${cm.ok}`);
if (cm.ok === true) failures.push("丢弃后仍能 commit ⇒ 状态没起作用 ✗");

const twice = await call("discard_stroke", { attempt_id: t.attempt_id });
console.log(`  重复丢弃 ⇒ ok=${twice.ok}`);
if (twice.ok === true) failures.push("重复丢弃竟然成功 ⇒ 状态流转没有守门 ✗");

const byStatus = await call("list_attempts", { status: "discarded" });
console.log(`  按 status=discarded ⇒ ${byStatus.count} 条`);
if (Number(byStatus.count) < 1) failures.push(`list_attempts 里查不到 discarded 记录 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 丢弃不落笔、状态为 discarded、之后不可提交、不可重复丢弃");
