#!/usr/bin/env node
// **`create_selection` 的 `mode` 必须能对已存在的 id 用**（第 773 轮；外部 `E1027`）
//  缺陷：`fold.rs` 无条件拒绝重复 id、不看 `mode` ⇒ `add`/`subtract`/`intersect` 永远不可达。
//  实测（修前）：`s1 + mode=subtract` ⇒ ok=false「选区 s1 已存在」。
//  断言：① 三种 mode 都必须 ok=true；② subtract 后的面积必须**小于**原面积；
//        ③ add 后的面积必须**大于**原面积；④ intersect 后的面积必须**小于等于**两者较小者。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-selection-mode-merge.mjs <base-url>"); process.exit(2); }
const doc = `sm-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "selection-mode-")));
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
const area = async (id) => {
  const r = await call("list_selections", {});
  const hit = (r.selections ?? []).find((s) => s.selection_id === id);
  return hit ? Number(hit.area ?? NaN) : NaN;
};

// 每轮**新建**一个选区，避免互相污染
const failures = [];
async function scenario(id, mode, shape, expect) {
  const a0 = await call("create_selection", { selection_id: id, shape: { kind: "rect", x: 20, y: 20, w: 200, h: 200 } });
  if (a0.ok !== true) { failures.push(`${id} 建不出来 ⇒ ${JSON.stringify(a0.context ?? {})} ✗`); return; }
  const r = await call("create_selection", { selection_id: id, mode, shape });
  const detail = (r.context ?? {}).detail;
  console.log(`  ${id} mode=${mode} ⇒ ok=${r.ok}${r.ok ? "" : "｜" + detail}`);
  if (r.ok !== true) { failures.push(`${id} 的 mode=${mode} 被拒（${detail}）⇒ 死参数 ✗`); return; }
  const got = Number(r.area ?? r.selected_area_px ?? NaN);
  if (!Number.isFinite(got)) { failures.push(`${id} mode=${mode} 成功后没给面积 ⇒ 无法验证合并 ✗`); return; }
  if (!expect(got)) failures.push(`${id} mode=${mode} 的面积=${got} 不符预期 ✗`);
  return got;
}

const BIG = 200 * 200;                                   // 40000
const SUB = { kind: "rect", x: 80, y: 80, w: 80, h: 80 }; // 6400
const ADD = { kind: "rect", x: 250, y: 20, w: 40, h: 40 }; // 1600，与原块不重叠

await scenario("s_sub", "subtract", SUB, (v) => v < BIG && v > 0);
await scenario("s_add", "add", ADD, (v) => v > BIG);
await scenario("s_int", "intersect", SUB, (v) => v <= 80 * 80);

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 三种 mode 都能对已存在的选区生效，面积按预期变化");
