#!/usr/bin/env node
// **版面规整度**判据（用户原话：布局不规整、各种杂乱、没有专业软件的设计感）。
//
// 量法（先看业界怎么做 ✓）：专业软件的间距落在**统一栅格**上（常见 4/8 的倍数 ✓），
// 字号只用**少数几档** ✓。所以这里统计查看器 CSS + 内联样式里的
// `gap / margin / padding / row-gap / column-gap` 与 `font-size` 的**取值分布** ✓：
//   * 间距值必须是 **4 的倍数** ✓（4/8/12/16… ✓；`0` 允许 ✓）；
//   * 间距的**不同取值数**不得超过 **10** ✓；字号不得超过 **6** 档 ✓；
//   * 超出就打印"哪一档、被用了多少次" ✓ ⇒ 这就是要修的名单 ✓。
//
// 为什么要这条判据：@用户 说"杂乱"是**观感** ✓，而观感必须能被量化才能改 ✓；
// 一旦"只允许 4 的倍数、且档位有限" ✓，版面自然会变得规整 ✓（这正是专业软件的做法 ✓）。
import { readFileSync } from "node:fs";
const source = readFileSync("crates/yanshi-http/src/viewer.rs", "utf8");
// 只看 HTML/CSS 那部分（排除 <script>，避免把 JS 里的数字当样式）
const withoutScript = source.replace(/<script[\s>][\s\S]*?<\/script>/g, "");
const collect = (pattern) => {
  const found = new Map();
  for (const match of withoutScript.matchAll(pattern)) {
    for (const raw of match[1].split(/\s+/)) {
      const value = raw.trim();
      const px = /^(-?\d+(?:\.\d+)?)px$/.exec(value);
      if (!px) continue;                       // 只看纯 px（忽略 auto/%/vh/var()）
      const number = Number(px[1]);
      found.set(number, (found.get(number) || 0) + 1);
    }
  }
  return found;
};
const spacing = collect(/(?:^|[;"'\s])(?:gap|margin|padding|row-gap|column-gap)(?:-top|-right|-bottom|-left)?\s*:\s*([^;"']+)/g);
const fontSizes = collect(/font-size\s*:\s*([^;"']+)/g);
// **字号不要求 4 的倍数** ✗（我第一版这么写是错的 ✓ —— 业界正文常是 11/12/14px ✓）；
// 字号只要求**档位少** ✓；**间距**才要求落在 4px 栅格上 ✓（这才是"规整"的来源 ✓）。
const report = (label, table, limit, allowZero = true, requireGrid = true) => {
  const values = [...table.keys()].sort((a, b) => a - b);
  const offenders = requireGrid
    ? values.filter((v) => !(allowZero && v === 0) && v % 4 !== 0)
    : [];
  const tooMany = values.length > limit;
  console.log(`  ${label}：${values.length} 档 ⇒ ${values.map((v) => v + "px(" + table.get(v) + ")").join(" ")}`);
  if (offenders.length) {
    console.log(`     ✗ 不在 4 的倍数上：${offenders.map((v) => v + "px(" + table.get(v) + ")").join(" ")}`);
  }
  if (tooMany) console.log(`     ✗ 档位过多（>${limit}）⇒ 应当收敛到少数几档`);
  return offenders.length === 0 && !tooMany;
};
console.log("  版面栅格检查（只统计 HTML/CSS，已排除 <script>）：");
const okSpacing = report("间距", spacing, 10);
const okFont = report("字号", fontSizes, 4, false, false);
if (okSpacing && okFont) {
  console.log("  ✓ 间距与字号都落在受限的档位上");
  process.exit(0);
}
console.log("  结论：版面不规整 ✗（按上面名单收敛档位）");
process.exit(1);
