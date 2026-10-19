#!/usr/bin/env node
// **数组颜色的两种域必须都生效**（第 948 轮；外部 §145 的整组测试曾被它坑）
//   `[r,g,b,a]`：全部 ≤ 1 ⇒ 线性；任一分量 > 1 ⇒ sRGB 字节 0-255
//   陷阱：`[200,200,200,1]` 前三个走字节域、第四个 `1` 是线性域（想表达不透明）
//   ⇒ 曾被当成「字节 1」= alpha 1/255 ≈ 全透明 ⇒ 合成后只剩背景 ⇒ 恒 255
//   ⇒ 于是 `[200,…]` 与 `[100,…]` 给出**同一个结果**（两个不同输入同一输出 = 最强 bug 证据）
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-color-array-domain.mjs <base-url>"); process.exit(2); }
const doc = `dom-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "color-domain-")));
// **前提失败必须 exit(3)**（第 1079 轮修，同 tool-medium-filter-covers-all）：
//   无服务时 `fetch` 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗（336 轮那条要求 3）
let created;
try {
  created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 100, height: 100 }),
  })).json();
} catch (error) {
  console.error(`  ⊘ 不可判：连不上服务（判据前提不成立，不是产品问题）：${String(error).slice(0, 80)}`);
  process.exit(3);
}
if (!created?.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
const call = (tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${created.token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

await call("create_layer", { layer_id: "a", name: "a" });
// [颜色, 标签, 期望的 R（±3）]
const cases = [
  [[0.5, 0.5, 0.5, 1], "线性 0.5", 188],
  [[200, 200, 200, 1], "字节 200", 200],
  [[100, 100, 100, 1], "字节 100", 100],
  [[40, 120, 60, 255], "字节 40/120/60", 40],
];
const failures = [];
let y = 5;
for (const [color, label, expect] of cases) {
  const r = await call("fill_region", { layer_id: "a", shape: { kind: "rect", x: 0, y, w: 100, h: 15 }, color });
  if (r.ok !== true) { failures.push(`${label}: 被拒 ⇒ ${JSON.stringify(r.context ?? {}).slice(0, 90)}`); y += 15; continue; }
  const px = await call("sample_colors", { points: [[50, y + 7]] });
  const got = (px.samples?.[0]?.rgba ?? []).slice(0, 3).join(",");
  const red = Number(got.split(",")[0]);
  console.log(`  ${label.padEnd(16)} ok=true｜实测=${got}（期望≈${expect}）`);
  if (!Number.isFinite(red) || Math.abs(red - expect) > 3) failures.push(`${label}: 期望 ≈${expect}，实测 ${got}`);
  y += 15;
}
// 恒等式：字节 200 与字节 100 **必须不同**（同输出 = alpha 被吃了）
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 数组颜色的线性域与字节域都生效（含 a=1 视为不透明）");
