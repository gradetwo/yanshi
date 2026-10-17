#!/usr/bin/env node
// **语义别名判据** ✓（第 1000+ 轮）：`hand_jitter` 这类组名，必须与它展开的 `.myb` 真名组**产出完全相同的笔触**。
//
// **为什么这样判** ✗：别名本身只是**一层映射** ✓ —— **∴ 它的正确性只有一条定义 ✓**：
//   **「给别名」与「给真名组」必须**逐字节等价**✗**（**∴ 否则要么映射错、要么漏项 ✓**）。
//   **∴ 用 `swatch_brush_strokes` 跑两次、比返回的图像字节 ✓** —— 这比"读回 base_value"更强 ✓
//   （**∴ 后者只证明写进去了，前者证明**画出来一样**✓✓**）。
//
// 用法：node scripts/tool-brush-aliases.mjs <server-base> <doc-id> <token>
import { readFileSync } from "node:fs";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-aliases.mjs <base> <doc> <token>");
  process.exit(3);
}
const swatch = async (params) => {
  const url = `${base}/api/tools/swatch_brush_strokes?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ brushes: ["oil-mop"], brush_params: params }),
  });
  return { status: reply.status, body: await reply.json() };
};
// 每组：别名 ⇒ 它应当等价的真名组（真名来自 hokusai 的 63 项规范名 ✓）
const GROUPS = [
  ["hand_jitter", { slow_tracking: 0.8, tracking_noise: 0.8, offset_by_random: 0.8 }],
  ["speed_gain", { speed1_gamma: 1.6, speed2_gamma: 1.6, speed1_slowness: 1.6, speed2_slowness: 1.6 }],
  ["color_variance", { change_color_h: 0.5, change_color_v: 0.5, change_color_hsl_s: 0.5 }],
  ["smudge_amount", { smudge: 0.7, smudge_length: 0.7, smudge_transparency: 0.7 }],
];
const strip = (value) => {
  const copy = JSON.parse(JSON.stringify(value));
  for (const key of ["timings", "duration_ms", "elapsed_ms"]) delete copy[key];
  return JSON.stringify(copy, Object.keys(copy).sort());
};
let bad = 0;
// ★ 结构断言：**读源码里的别名表，逐项比对** ✓（第 1000+ 轮 ✓）
//   **为什么必须加** ✗：实测发现**只比像素**有盲区 —— 把 `hand_jitter` 的展开删掉一项
//   （`offset_by_random`），**判据仍然绿** ✗ ⇒ **∵ 那项在 `oil-mop` 上本来就不生效 ✓**
//   ⇒ **∴ 像素等价只能证明「生效的项一致」✗，证明不了「展开完整」✗** ⇒ **∴ 这里直接读源码 ✓**。
{
  const source = readFileSync(new URL("../crates/yanshi-server/src/tools.rs", import.meta.url), "utf8");
  for (const [alias, real] of GROUPS) {
    const pattern = new RegExp(`"${alias}"\\s*=>\\s*Some\\(&\\[([^\\]]*)\\]\\)`);
    const hit = source.match(pattern);
    if (!hit) {
      console.log(`  ✗ ${alias}：源码里找不到别名声明`);
      bad += 1;
      continue;
    }
    const declared = hit[1].split(",").map((entry) => entry.trim().replace(/^"|"$/g, "")).filter(Boolean).sort();
    const expected = Object.keys(real).sort();
    const same = JSON.stringify(declared) === JSON.stringify(expected);
    console.log(`  ${same ? "✓" : "✗"} ${alias} 的展开项：源码 ${JSON.stringify(declared)}｜期望 ${JSON.stringify(expected)}`);
    if (!same) bad += 1;
  }
}
for (const [alias, real] of GROUPS) {
  const viaAlias = await swatch({ [alias]: 0.8 });
  const value = Object.values(real)[0];
  const viaReal = await swatch(Object.fromEntries(Object.keys(real).map((key) => [key, value])));
  if (viaAlias.status !== 200 || viaReal.status !== 200) {
    console.log(`  ✗ ${alias}: swatch 失败（${viaAlias.status} / ${viaReal.status}）`);
    bad += 1;
    continue;
  }
  const same = strip(viaAlias.body) === strip(viaReal.body);
  console.log(`  ${same ? "✓" : "✗"} ${alias} ≡ ${Object.keys(real).join(" + ")}（笔触逐字节${same ? "相同" : "不同"}）`);
  if (!same) bad += 1;
}
if (bad) {
  console.log(`  ✗ 语义别名有 ${bad} 组与真名组不等价`);
  process.exit(1);
}
console.log("  ✓ 四组语义别名都与各自展开的真名组逐字节等价");
