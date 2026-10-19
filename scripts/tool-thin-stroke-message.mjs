#!/usr/bin/env node
// **细线不落墨时的报错文案判据**（第 731/732 轮）：
//   ① `calligraphy` 在 size=3 必须**失败**（当前是产品行为，若哪天修好了这条会红 ⇒ 提醒我更新）
//   ② **文案必须指向 hardness／抗锯齿**，不能指向 `dabs_per_basic_radius` 的"间距趋于无穷"
//      （旧文案那样写会把人引去查 dpar，而 dpar 不是原因）
//   ③ 文案必须给出**可执行**的动作（含「加大 size」）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-thin-stroke-message.mjs <base-url>"); process.exit(2); }
const doc = `ts-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "thin-stroke-")));
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

await call("create_layer", { layer_id: "p", name: "p" });
const r = await call("brush_stroke", {
  layer_id: "p", brush: "calligraphy", size: 3,
  color: [0.05, 0.05, 0.05, 1.0], points: [[40, 150, 0.9], [260, 150, 0.9]], preview: false,
});
const detail = String((r.context || {}).detail ?? "");
console.log(`  size=3 calligraphy ⇒ ok=${r.ok}`);
console.log(`  文案含 hardness=${/hardness/i.test(detail)}｜含抗锯齿=${detail.includes("抗锯齿")}｜含加大 size=${detail.includes("加大 size")}`);

const failures = [];
if (r.ok !== false) failures.push("size=3 的 calligraphy 居然成功了 ⇒ 产品行为变了，请更新这条判据（不是产品失败）");
// **只在"正向断言"时才判红**：新文案末尾有一句"不会趋于无穷"是否定句，不该被判红。
// 所以要求：出现「趋于无穷」时，同一句里必须也出现否定词（不会／不是／不）。
if (/\u8d8b\u4e8e\u65e0\u7a77/.test(detail)) {
  const idx = detail.indexOf("\u8d8b\u4e8e\u65e0\u7a77");
  const near = detail.slice(Math.max(0, idx - 20), idx);
  if (!/\u4e0d|\u6ca1\u6709/.test(near)) {
    failures.push("文案在正向断言「趋于无穷」⇒ 那是我第 731 轮已证伪的解释 ✗");
  }
}
// **不能只 grep `hardness`**：文案里它出现两次（一次是根因、一次是否定 dpar 时的引用），
// 只改一处不会红（第 733 轮变异就是这么漏掉的）。改成要求**根因那半句**的具体措辞。
if (!/\u628a\s*`?hardness`?\s*\u8c03\u6210/u.test(detail) && !/\u8c03\u6210\*\*\u8d1f\u6570\*\*/u.test(detail)) {
  failures.push("文案没有「把 hardness 调成负数」这句根因 ✗");
}
if (!detail.includes("抗锯齿")) failures.push("文案没提抗锯齿分支 ✗");
if (!detail.includes("加大 size")) failures.push("文案没给可执行动作（加大 size）✗");

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 文案指向 AA/hardness，不含已证伪的「趋于无穷」，且给了可执行动作");
