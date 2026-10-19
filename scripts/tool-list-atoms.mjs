#!/usr/bin/env node
// **`list_atoms` 必须能按 label 筛、按 seq 倒序、limit 生效**（需求 v2 §4.3）
//   ⇒ 外部复测方说真实卡点是「画完了找不到哪一笔坏了」⇒ 他需要按「我怎么记得那笔」去查
//   ⇒ 所以判据要验三件事：标签筛得准、不筛就是全部、顺序是新的在前
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-list-atoms.mjs <base-url>"); process.exit(2); }
const doc = `la-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "list-atoms-")));
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
const A = "鼻子-轮廓", B = "眼睛-高光";
await call("brush_stroke", { layer_id: "p", brush: "100%_Opaque", points: [[20,20,1],[60,60,1]], size: 10, color: {r:255,g:0,b:0,a:255}, label: A });
await call("brush_stroke", { layer_id: "p", brush: "100%_Opaque", points: [[100,20,1],[140,60,1]], size: 10, color: {r:0,g:255,b:0,a:255}, label: B });

const failures = [];


// ⑦ 缺省只列笔迹（外部 §110：seq=2 是 create_layer，没有笔刷名，会被误当成"同一支笔有一笔记不上"）
const def = await call("list_atoms", { limit: 50 });
const defKinds = [...new Set((def.atoms ?? []).map((a) => a.kind))];
const withNon = await call("list_atoms", { limit: 50, include_non_stroke: true });
console.log(`  缺省种类=${defKinds.join(",")}｜含非笔迹=${(withNon.atoms ?? []).length} 条`);
if (defKinds.some((k) => k !== "import_image")) failures.push(`缺省应只列笔迹，实测含 ${defKinds.join(",")} ✗`);
if ((def.atoms ?? []).some((a) => a.brush == null)) failures.push(`缺省的笔迹里出现了 brush 为空的项 ⇒ 非笔迹混进来了 ✗`);
if ((withNon.atoms ?? []).length <= (def.atoms ?? []).length) failures.push(`include_non_stroke=true 应比缺省多，实测 ${(withNon.atoms ?? []).length} 不大于 ${(def.atoms ?? []).length} ✗`);

// ⑥ 按 bbox 筛 ⇒ 只保留与该框相交的笔迹（外部 §109 要求的"在画面这个区域里"）
const nearBox = await call("list_atoms", { bbox: [0, 0, 60, 60] });
const farBox = await call("list_atoms", { bbox: [150, 150, 40, 40] });
const emptyBox = await call("list_atoms", { bbox: [0, 0, 3, 3] });
console.log(`  bbox=[0,0,60,60]⇒ ${(nearBox.atoms ?? []).length}｜bbox=[150,150,40,40]⇒ ${(farBox.atoms ?? []).length}｜bbox=[0,0,3,3]⇒ ${(emptyBox.atoms ?? []).length}`);
if ((nearBox.atoms ?? []).length < 1) failures.push(`bbox 覆盖第一笔，应至少 1 条，实测 ${(nearBox.atoms ?? []).length} 条 ✗`);
if ((emptyBox.atoms ?? []).length !== 0) failures.push(`bbox 与任何笔迹都不相交，应 0 条，实测 ${(emptyBox.atoms ?? []).length} 条 ⇒ 区域筛选退化成全返回 ✗`);

// ⑤ 按 brush 筛 ⇒ 命中（外部 §109 实测它曾恒 0 条：笔刷名在 payload.source 里，不在顶层）
const byBrush = await call("list_atoms", { brush: "100%_Opaque" });
const listBrush = byBrush.atoms ?? [];
console.log(`  brush=100%_Opaque ⇒ ${listBrush.length} 条｜brush=${JSON.stringify(listBrush[0]?.brush)}`);
if (listBrush.length < 1) failures.push(`按 brush 筛应至少 1 条，实测 ${listBrush.length} 条 ⇒ 笔刷名没被读到 ✗`);
const noBrush = await call("list_atoms", { brush: "nonexistent-xyz" });
if ((noBrush.atoms ?? []).length !== 0) failures.push(`不存在的 brush 应得 0 条，实测 ${(noBrush.atoms ?? []).length} 条 ✗`);

// ① 按 label 筛 ⇒ 只 1 条，且标签对
const byA = await call("list_atoms", { label: A });
const listA = byA.atoms ?? [];
console.log(`  label=${A} ⇒ ${listA.length} 条`);
if (listA.length !== 1) failures.push(`按 label 筛应得 1 条，实测 ${listA.length} 条 ⇒ 过滤没生效或过宽 ✗`);
if (listA.length === 1 && listA[0].label !== A) failures.push(`返回项的 label 是 ${JSON.stringify(listA[0].label)}，应为 ${A} ✗`);

// ② 不筛 ⇒ 至少这两笔，且倒序（seq 递减）
const all = await call("list_atoms", { limit: 50 });
const listAll = all.atoms ?? [];
console.log(`  不筛 ⇒ ${listAll.length} 条；前两个 seq = ${listAll.slice(0,2).map((x) => x.seq).join(",")}`);
if (listAll.length < 2) failures.push(`不筛应至少 2 条，实测 ${listAll.length} 条 ✗`);
for (let i = 1; i < listAll.length; i += 1) {
  if (listAll[i - 1].seq <= listAll[i].seq) {
    failures.push(`结果不是按 seq 倒序（第 ${i-1} 条 ${listAll[i-1].seq} 不大于第 ${i} 条 ${listAll[i].seq}）✗`);
    break;
  }
}

// ③ limit 生效
const one = await call("list_atoms", { limit: 1 });
console.log(`  limit=1 ⇒ ${(one.atoms ?? []).length} 条`);
if ((one.atoms ?? []).length !== 1) failures.push(`limit=1 应得 1 条，实测 ${(one.atoms ?? []).length} 条 ✗`);

// ④ 不存在的 label ⇒ 0 条（不许退化成"全返回"）
const none = await call("list_atoms", { label: "不存在的标签-xyz" });
console.log(`  不存在的 label ⇒ ${(none.atoms ?? []).length} 条`);
if ((none.atoms ?? []).length !== 0) failures.push(`不存在的 label 应得 0 条，实测 ${(none.atoms ?? []).length} 条 ⇒ 过滤退化成全返回 ✗`);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 标签筛得准、不筛是全部、按 seq 倒序、limit 生效、不存在的标签返回 0 条");
