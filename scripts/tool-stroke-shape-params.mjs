#!/usr/bin/env node
// **`stroke_taper` / `pressure_shape` 必须真的改变输出**（第 612 轮的根因守卫）
//   根因：引擎的启用条件要 `state.stroke_total_length > 1.0`，而这个字段从来没被赋值 ⇒ 恒为 0
//   ⇒ 两个参数传了也完全不生效（四种取值像素数全是 2438）。修法是落笔前把整笔弧长写进去。
//   ① taper>0 与基线不同 ② pressure 正负都不同 ③ 正负压力互相不同（符号真的起作用）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-stroke-shape-params.mjs <base-url>"); process.exit(2); }
const doc = `sp-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "shape-params-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "p", name: "p" });
const failures = [];
const stroke = (y, extra = {}) => call("brush_stroke", {
  layer_id: "p", brush: "calligraphy", size: 20, color: [0.1, 0.3, 0.9, 1.0],
  points: [[80, y, 0.9], [160, y, 0.9], [240, y, 0.9]], preview: false, ...extra,
});

const a = await stroke(80);
if (!a.ok || !(Number(a.painted_pixels) > 0)) {
  console.error("判据自身前提不成立：基线笔画没落墨（不是产品问题）"); process.exit(3);
}
const taper = await stroke(160, { brush_params: { stroke_taper: 0.8 } });
const pos = await stroke(240, { brush_params: { pressure_shape: 0.8 } });
const neg = await stroke(320, { brush_params: { pressure_shape: -0.8 } });
console.log(`  基线 ${a.painted_pixels}｜taper ${taper.painted_pixels}｜pressure+0.8 ${pos.painted_pixels}｜-0.8 ${neg.painted_pixels}`);

if (Number(taper.painted_pixels) === Number(a.painted_pixels)) {
  failures.push(`stroke_taper=0.8 的输出与基线相同（都是 ${a.painted_pixels}）⇒ 参数没生效 ✗`);
}
if (Number(pos.painted_pixels) === Number(a.painted_pixels)) {
  failures.push(`pressure_shape=+0.8 的输出与基线相同（都是 ${a.painted_pixels}）⇒ 参数没生效 ✗`);
}
if (Number(neg.painted_pixels) === Number(a.painted_pixels)) {
  failures.push(`pressure_shape=-0.8 的输出与基线相同（都是 ${a.painted_pixels}）⇒ 参数没生效 ✗`);
}
// 符号必须真的起作用（正负给不同结果）——只验"与基线不同"的话，一个恒定的扰动也能骗过
if (Number(pos.painted_pixels) === Number(neg.painted_pixels)) {
  failures.push(`pressure_shape 正负给出相同结果（都是 ${pos.painted_pixels}）⇒ 符号没起作用 ✗`);
}

// ⑤ 意图参数（顶层 stroke_taper / pressure_shape）与 brush_params 两条路必须等价
// **两笔必须同一条路径**（第 613 轮：第一次我用了 y=400 与 y=80 两个位置，y=400 在画布边缘被裁，
// 于是"两条路不同"是假的——那是我自己的测试写错，不是产品问题）
const tIntent = await stroke(120, { stroke_taper: 0.8 });
const tParam = await stroke(120, { brush_params: { stroke_taper: 0.8 } });
console.log(`  意图 stroke_taper ${tIntent.painted_pixels}｜brush_params stroke_taper ${tParam.painted_pixels}`);
if (Number(tIntent.painted_pixels) !== Number(tParam.painted_pixels)) {
  failures.push(`两条路结果不同（${tIntent.painted_pixels} vs ${tParam.painted_pixels}）⇒ 意图参数没走同一条映射 ✗`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 收锋与压力曲线都真的改变了输出，且正负压力给出不同结果");
