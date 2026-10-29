#!/usr/bin/env node
// **细线不落墨时的报错文案判据**（第 731/732 轮；P2-13 更新；反馈期 1 更新）：
//   ① `calligraphy` 在 size=3、220px 路径上必须**成功**（P2-13 修了 AA 钳制：
//      `hardness_new <= 0` 时跳过 AA 调整，小 size 也能落墨；本条旧版写的是
//      “size=3 必须失败，若哪天修好了这条会红 ⇒ 提醒我更新”——现在就是那一天）。
//   ② **短路径兜底**（size=3、路径总长 0.4px ⇒ 撑不满一个 dab 间距）：
//      反馈期 1 之前是真 0 墨拒绝，文案必须指向**路径太短**；期 1 之后改为
//      兜底落墨成功，诊断事实（路径太短／dab 间距）降为回执 `warnings` 里的提示，
//      不再是拒绝理由。旧版“必须 ok=false”的断言随之反转（与 ① 同例：
//      “若哪天修好了这条会红 ⇒ 提醒我更新”——现在就是那一天）。
//   ③ 提示里仍不许把 0 墨赖给抗锯齿/hardness 变负（那档原因 P2-13 已修掉）。
//      “想要更长的笔触请加长路径”仍是提示里可执行的动作。
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

// ② 短路径兜底（0.4px 路径）：反馈期 1 之后必须成功落墨，诊断事实进 warnings 提示。
const r = await call("brush_stroke", {
  layer_id: "p", brush: "calligraphy", size: 3,
  color: [0.05, 0.05, 0.05, 1.0], points: [[40, 150, 0.9], [40.4, 150, 0.9]], preview: false,
});
const hint = (r.warnings || []).join(" ");
console.log(`  size=3 calligraphy 0.4px 路径 ⇒ ok=${r.ok}｜painted=${r.painted_pixels}`);
console.log(`  提示含 路径太短=${hint.includes("路径太短")}｜含 dab 间距=${hint.includes("dab 间距")}｜含加长路径=${hint.includes("加长路径")}｜含调成负数=${hint.includes("调成负数")}`);
if (r.ok !== true) failures.push("0.4px 路径的 calligraphy 现在应当兜底成功（反馈期 1：短路径落一枚 dab，不再拒绝）");
if (!(Number(r.painted_pixels ?? 0) > 0)) failures.push("0.4px 路径兜底后必须真的落墨（painted_pixels > 0）");
if (!hint.includes("路径太短")) failures.push("兜底提示必须保留诊断事实「路径太短」");
if (!hint.includes("dab 间距")) failures.push("兜底提示必须保留诊断事实「dab 间距」");
if (!hint.includes("加长路径")) failures.push("提示必须给可执行动作（加长路径）");
// 旧文案把 0 墨赖给 AA（“size 太小（抗锯齿把 hardness 压成负数）”、“把 hardness 调成负数”）
// ⇒ 新提示里这三种说法都不许再出现。
for (const stale of ["调成负数", "压成负数", "size 太小"]) {
  if (hint.includes(stale)) failures.push(`提示不能再把短路径赖给旧 AA 原因（出现「${stale}」）`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ size=3 落墨正常；短路径兜底落墨，提示保留路径太短／dab 间距诊断，不再赖抗锯齿");
