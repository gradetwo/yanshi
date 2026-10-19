#!/usr/bin/env node
// **`brush_stroke` 的 `label` 要能**写进原子**且**读得回来**** ✓（外部需求 v2 §4.2 ✓）
//   ⇒ **∴ 判据**✗：画两笔（**∴ 一笔带 label、一笔不带 ✓**）
//      ⇒ **∴ 带的那笔** `get_atom.label` **必须**等于传入值**✗**
//      ⇒ **∴ 不带的那笔**必须**是 null**✗**（**∴ 不许**被上一笔「泄漏」到 ✓**）
//   ⇒ **∴ 变异**✗：**∴ 让 `commit` 忽略 `pending_label`**✗ ⇒ **∴ 第一条**必红**✗**
//
// **∴ 为什么**要验「不带的那笔」**✗**：**∴ `pending_label`** 是**一次性状态**✗** ⇒
// **∴ 若**忘了 `take()`**✗** ⇒ **∴ 标签**会**粘**在**后面所有笔上**✗**（**∴ 那**是**状态泄漏 ✓**）。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-stroke-label.mjs <base-url>"); process.exit(2); }
const doc = `lbl-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "stroke-label-")));
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
const LABEL = "NOSE-CONTOUR";
const stroke = (points, extra = {}) => call("brush_stroke", {
  layer_id: "p", brush: "100%_Opaque", points, size: 12,
  color: { r: 255, g: 0, b: 0, a: 255 }, ...extra,
});

const failures = [];
const withLabel = await stroke([[20, 20, 1], [80, 80, 1]], { label: LABEL });
if (withLabel.ok !== true) failures.push(`带 label 的那笔失败了 ⇒ ${JSON.stringify(withLabel).slice(0, 140)}`);
const plain = await stroke([[120, 20, 1], [180, 80, 1]]);

// **① 带 label ⇒ 读回就是它** ✓
const a1 = withLabel.atom_id ? await call("get_atom", { atom_id: withLabel.atom_id }) : null;
const got1 = a1 ? a1.label : undefined;
console.log(`  带 label ⇒ atom=…${String(withLabel.atom_id).slice(-6)}｜label=${JSON.stringify(got1 ?? null)}`);
if (got1 !== LABEL) {
  failures.push(`带 label 的那笔读回是 ${JSON.stringify(got1 ?? null)}，应为 ${JSON.stringify(LABEL)} ⇒ 标签没写进原子或读不出来 ✗`);
}

// **③ 两笔必须是**两个原子**✗**（**∴ 否则**第二条断言**在**验**同一个**原子**✗ ⇒ **∴ 假绿**✗**）
if (!withLabel.atom_id || !plain.atom_id) {
  failures.push(`有一笔没返回 atom_id（带=${withLabel.atom_id}，不带=${plain.atom_id}）✗`);
} else if (withLabel.atom_id === plain.atom_id) {
  failures.push(`两笔拿到同一个 atom_id ${withLabel.atom_id} ⇒ 第二笔没真的落笔 ⇒ 下面那条'不泄漏'是在验同一个原子（假绿）✗`);
}

// **② 不带 ⇒ 必须是 null** ✓（**∴ 防**状态泄漏**✗**）
const a2 = plain.atom_id ? await call("get_atom", { atom_id: plain.atom_id }) : null;
const got2 = a2 ? a2.label : undefined;
console.log(`  不带 label ⇒ atom=…${String(plain.atom_id).slice(-6)}｜label=${JSON.stringify(got2 ?? null)}`);
if (got2 != null) {
  failures.push(`不带 label 的那笔读回是 ${JSON.stringify(got2)}，应为 null ⇒ 标签泄漏到下一笔 ✗`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log(`  ✓ label 写进原子且读得回来（${LABEL}），且不泄漏到下一笔`);
