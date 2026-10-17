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
  ["blend_modes", "crates/yanshi-render/src/blend.rs", /pub const NAMES: \[(?:&(?:'static )?)?str; (\d+)\]/],
  ["filters", "crates/yanshi-render/src/filter.rs", /pub const FILTER_NAMES: \[&str; (\d+)\]/],
  ["adjustments", "crates/yanshi-render/src/filter.rs", /pub const ADJUSTMENT_NAMES: \[&str; (\d+)\]/],
  ["retouch_kinds", "crates/yanshi-render/src/object.rs", /pub const RETOUCH_KINDS: \[(?:&(?:'static )?)?str; (\d+)\]/],
  ["liquify_kinds", "crates/yanshi-render/src/object.rs", /pub const LIQUIFY_KINDS: \[(?:&(?:'static )?)?str; (\d+)\]/],
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
// ★ 介质那条路的参数：**读 `medium_stroke` 的 schema，与工具返回的 medium_params.names 逐项比** ✓
//   **∴ 为什么读源码** ✗：这样 Rust 侧加了参数而这里没跟上（或反过来）都会红 ✓（**∴ 硬编码清单必漂移 ✗**）
{
  const src = readFileSync(new URL("../crates/yanshi-server/src/tools.rs", import.meta.url), "utf8");
  const start = src.indexOf('name: "medium_stroke"');
  if (start < 0) { console.log("  ✗ 源码里找不到 medium_stroke"); process.exit(3); }
  const next = src.indexOf('name: "', start + 10);
  const block = src.slice(start, next > 0 ? next : start + 20000);
  const declared = [...block.matchAll(/param!\(\s*"([a-z_]+)"/g)].map((m) => m[1]).sort();
  if (!declared.length) { console.log("  ✗ medium_stroke 的 schema 里没解析出参数 ⇒ 判据无法判定"); process.exit(3); }
  const reported = [...(body.medium_params?.names ?? [])].sort();
  check(JSON.stringify(declared) === JSON.stringify(reported),
    `medium_params 与 medium_stroke schema 一致：工具 ${JSON.stringify(reported)}｜源码 ${JSON.stringify(declared)}`);
}

// ★ 「需要额外参数的滤镜」两向检查 ✓（第 1000+ 轮 ✓）：
//   ① 清单里声明的每个参数，源码里必须真的有 `params.get("<名>")` ✓；
//   ② 源码里 `dehaze` 读 `air` ⇒ 清单也必须声明它 ✓（**∴ 否则 agent 只会看到"dehaze 可用"✗**）。
{
  const fsrc = readFileSync(new URL("../crates/yanshi-render/src/filter.rs", import.meta.url), "utf8");
  const needs = body.filters_needing_params ?? {};
  for (const [name, params] of Object.entries(needs)) {
    for (const p of params) {
      check(fsrc.includes(`params.get("${p}")`), `${name} 的 ${p}：清单声明了，源码里确有 params.get("${p}")`);
    }
  }
  check(
    !fsrc.includes('params.get("air")') || (needs.dehaze ?? []).includes("air"),
    `源码里 dehaze 读 air ⇒ 清单必须声明 dehaze: [air]（实际 ${JSON.stringify(needs.dehaze)}）`,
  );
}

// 示例介质不能混进成品介质 ✓（**真名是 `example` ✓，不是文件名 `example-dab` ✗**）
check(
  !(body.mediums ?? []).includes("example") && (body.example_mediums ?? []).includes("example"),
  "示例介质 example 只在 example_mediums 里，不在 mediums 里",
);
// ★ 介质清单：**从引擎自己的错误消息里取候选** ✓（最不漂移 —— 那是服务端在用的那份 ✓）
{
  const probe = await fetch(
    `${base}/api/tools/medium_stroke?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ layer_id: "layer_default", medium: "__definitely_not_a_medium__", points: [[10, 10]] }),
    },
  );
  const body2 = await probe.json();
  const detail = body2?.context?.detail ?? "";
  const after = detail.split("可用：")[1] ?? "";
  const candidates = after.split("/").map((entry) => entry.trim()).filter(Boolean);
  if (!candidates.length) {
    console.log(`  ✗ 拿不到介质候选（错误消息：${detail.slice(0, 120)}）⇒ 判据无法判定`);
    process.exit(3);
  }
  const toolAll = [...(body.mediums ?? []), ...(body.example_mediums ?? [])].sort();
  const engineAll = [...candidates].sort();
  check(
    JSON.stringify(toolAll) === JSON.stringify(engineAll),
    `介质清单与引擎候选一致：工具 ${JSON.stringify(toolAll)}｜引擎 ${JSON.stringify(engineAll)}`,
  );
  check(
    !(body.mediums ?? []).some((entry) => entry.includes("example")),
    "示例介质没有混进 mediums",
  );
}
if (bad) {
  console.log(`  ✗ 能力清单有 ${bad} 处与引擎常量不一致`);
  process.exit(1);
}
console.log("  ✓ 能力清单与引擎常量逐项一致（含「示例介质不混入成品」这层语义）");
