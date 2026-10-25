#!/usr/bin/env node
// **细线不落墨时的报错文案判据**（第 731/732 轮；P2-13 更新）：
//   ① `calligraphy` 在 size=3、220px 路径上必须**成功**（P2-13 修了 AA 钳制：
//      `hardness_new <= 0` 时跳过 AA 调整，小 size 也能落墨；本条旧版写的是
//      “size=3 必须失败，若哪天修好了这条会红 ⇒ 提醒我更新”——现在就是那一天）。
//   ② **真 0 墨**（size=3、路径总长 0.4px ⇒ 撑不满一个 dab 间距）的文案必须
//      指向**路径太短**，不能再指向抗锯齿/hardness 变负（那档原因已修掉）。
//   ③ 文案必须给出**可执行**的动作（含「加长路径」）。
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
const failures = [];

// ① size=3 长路径必须落墨（P2-13 已修）。
const okStroke = await call("brush_stroke", {
  layer_id: "p", brush: "calligraphy", size: 3,
  color: [0.05, 0.05, 0.05, 1.0], points: [[40, 150, 0.9], [260, 150, 0.9]], preview: false,
});
console.log(`  size=3 calligraphy 长路径 ⇒ ok=${okStroke.ok}｜painted=${okStroke.painted_pixels}`);
if (okStroke.ok !== true || !(Number(okStroke.painted_pixels ?? 0) > 0)) {
  failures.push(`size=3 的 calligraphy 长路径应当落墨（P2-13 已修 AA 钳制）：${JSON.stringify(okStroke).slice(0, 160)}`);
}

// ② 真 0 墨（0.4px 路径）的文案必须指向路径太短，不能再赖抗锯齿。
const r = await call("brush_stroke", {
  layer_id: "p", brush: "calligraphy", size: 3,
  color: [0.05, 0.05, 0.05, 1.0], points: [[40, 150, 0.9], [40.4, 150, 0.9]], preview: false,
});
const detail = String((r.context || {}).detail ?? "");
console.log(`  size=3 calligraphy 0.4px 路径 ⇒ ok=${r.ok}`);
console.log(`  文案含 路径太短=${detail.includes("路径太短")}｜含加长路径=${detail.includes("加长路径")}｜含调成负数=${detail.includes("调成负数")}`);
if (r.ok !== false) failures.push("0.4px 路径的 calligraphy 应当 0 墨失败（撑不满一个 dab 间距）");
if (!detail.includes("路径太短")) failures.push("真 0 墨的文案必须指向「路径太短」");
if (!detail.includes("加长路径")) failures.push("文案必须给可执行动作（加长路径）");
// 旧文案把 0 墨赖给 AA（“size 太小（抗锯齿把 hardness 压成负数）”、“把 hardness 调成负数”）
// ⇒ 新文案里这三种说法都不许再出现（“不是抗锯齿：P2-13 已修……”那句解释除外，
// 它说的是“跳过调整”，不含这三种说法）。
for (const stale of ["调成负数", "压成负数", "size 太小"]) {
  if (detail.includes(stale)) failures.push(`文案不能再把 0 墨赖给旧 AA 原因（出现「${stale}」）`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ size=3 落墨正常；真 0 墨的文案指向路径太短，不再赖抗锯齿");
