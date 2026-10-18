#!/usr/bin/env node
// **`create_layer(..., if_exists="reuse")` 必须真的复用，缺省仍报错** ✓
//
// **∴ 为什么需要它 ✗**：报告 §31① —— 他们每次启动都 `create_layer("subject_paint")` ✓
//   并用 `try/except: pass` 吞掉失败 ✓ ⇒ **∴ 于是 stderr 每次都打印 `precondition_failed` ✗**
//   ⇒ **∴ 而它与"真失败"✗ 无法区分 ✓** ⇒ **∴ 所以给一个**显式**的复用选项 ✓**
//
// **∴ 判据 ✓**：
//   ① 首次建 ⇒ `ok: true` ✓
//   ② **缺省**重名 ⇒ `ok: false` ＋ `error_code: precondition_failed` ✓（**∴ 不改既有语义 ✓**）
//   ③ `if_exists="reuse"` ⇒ `ok: true` ＋ `reused: true` ✓
//   ④ **图层数不变**✗（**∴ 关键 ✓：复用不等于又建一层 ✓**）
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-create-layer-if-exists.mjs <base-url>"); process.exit(3); }

const boot = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: `ifexists_${Date.now()}`, width: 200, height: 200 }),
});
const bootBody = await boot.json().catch(() => ({}));
const token = bootBody.token;
if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(3); }
const doc = bootBody.doc_id ?? bootBody.doc ?? "ifexists";

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) });
  return await reply.json();
};

let bad = 0;
const first = await call("create_layer", { layer_id: "sp", name: "subject_paint" });
console.log(`  ${first.ok === true ? "✓" : "✗"} 首次建 sp ⇒ ok=${first.ok}`);
if (first.ok !== true) bad += 1;

const dup = await call("create_layer", { layer_id: "sp", name: "subject_paint" });
const dupOk = dup.ok === false && dup.error_code === "precondition_failed";
console.log(`  ${dupOk ? "✓" : "✗"} **缺省**重名 ⇒ ok=${dup.ok}｜error_code=${dup.error_code}（**必须 false ＋ precondition_failed**）`);
if (!dupOk) bad += 1;

const reused = await call("create_layer", { layer_id: "sp", name: "subject_paint", if_exists: "reuse" });
const reuseOk = reused.ok === true && reused.reused === true;
console.log(`  ${reuseOk ? "✓" : "✗"} if_exists=reuse ⇒ ok=${reused.ok}｜reused=${reused.reused}（**必须 true ＋ true**）`);
if (!reuseOk) bad += 1;

const listed = await call("list_layers", {});
const count = listed.count;
const countOk = count === 2;
console.log(`  ${countOk ? "✓" : "✗"} 图层数 = ${count}（**必须 2**：默认层 ＋ sp）⇒ 复用没有新建`);
if (!countOk) bad += 1;

if (bad) { console.log(`  ✗ ${bad} 处不符（create_layer 的 if_exists 契约）`); process.exit(1); }
console.log("  ✓ create_layer：缺省重名报错、reuse 真复用且不新建");
