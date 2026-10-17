#!/usr/bin/env node
// **能力清单判据** ✓（第 1000+ 轮）：`list_engine_capabilities` 报的数字，必须与**引擎源码里的常量**一致。
//
// **为什么这样设计** ✗：如果判据里也写死 11／14／12／4／3，那它只能证明"工具没坏"✗，
// 不能证明"工具**没漂移**"✗ —— **∴ 真正的风险是**有人加了一种修图，工具还报 4**✗✓
// ⇒ **∴ 本判据直接读源码，从 `[&str; N]` / `[&'static str; N]` 里取 N，再与工具的输出比** ✓✓
//
// 用法：node scripts/tool-engine-capabilities.mjs <server-base> <doc-id> <token>
import { readFileSync } from "node:fs";
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-engine-capabilities.mjs <base> <doc> <token>");
  process.exit(3);
}
const url = `${base}/api/tools/list_engine_capabilities?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: "{}" });
if (reply.status !== 200) {
  console.log(`  ✗ list_engine_capabilities 失败（HTTP ${reply.status}）`);
  process.exit(1);
}
const body = await reply.json();
// 从源码里抠常量长度 ✓（每条都要求**恰好一处**匹配 ✗ ⇒ 找不到就退 3，不假装通过 ✓）
const expected = (file, pattern) => {
  const text = readFileSync(new URL(`../${file}`, import.meta.url), "utf8");
  // **`matchAll` 要求全局正则** ✗（缺 `g` 会 TypeError）⇒ 这里补上 ✓
  const hits = [...text.matchAll(new RegExp(pattern.source, "g"))];
  if (hits.length !== 1) {
    console.log(`  ✗ 在 ${file} 里找不到唯一的常量声明（命中 ${hits.length} 处）⇒ 判据无法判定`);
    process.exit(3);
  }
  return Number(hits[0][1]);
};
const wants = [
  ["blend_modes", "crates/yanshi-render/src/blend.rs", /pub const NAMES: \[&'static str; (\d+)\]/],
  ["filters", "crates/yanshi-render/src/filter.rs", /pub const FILTER_NAMES: \[&str; (\d+)\]/],
  ["adjustments", "crates/yanshi-render/src/filter.rs", /pub const ADJUSTMENT_NAMES: \[&str; (\d+)\]/],
  ["retouch_kinds", "crates/yanshi-render/src/object.rs", /pub const RETOUCH_KINDS: \[&'static str; (\d+)\]/],
  ["liquify_kinds", "crates/yanshi-render/src/object.rs", /pub const LIQUIFY_KINDS: \[&'static str; (\d+)\]/],
];
let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};
for (const [key, file, pattern] of wants) {
  const n = expected(file, pattern);
  const reported = body.counts?.[key];
  const listed = (body[key] ?? []).length;
  check(
    reported === n && listed === n,
    `${key}: 工具报 ${reported}、列表长 ${listed}、源码常量 ${n}${reported === n && listed === n ? "" : "（不一致）"}`,
  );
}
// 笔刷动力学：工具报的数必须与它自己列出的名字数一致（63 由 hokusai 决定，不在本仓 ✗）
check(
  body.counts?.brush_settings === (body.brush_settings ?? []).length,
  `brush_settings: 工具报 ${body.counts?.brush_settings}、列出 ${(body.brush_settings ?? []).length}（应一致）`,
);
// 示例介质不能混进成品介质 ✓
check(
  !(body.mediums ?? []).includes("example-dab") && (body.example_mediums ?? []).includes("example-dab"),
  "example-dab 只在 example_mediums 里，不在 mediums 里",
);
if (bad) {
  console.log(`  ✗ 能力清单有 ${bad} 处与引擎常量不一致`);
  process.exit(1);
}
console.log("  ✓ 能力清单与引擎常量逐项一致（含「示例介质不混入成品」这层语义）");
