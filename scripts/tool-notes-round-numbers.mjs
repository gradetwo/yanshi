#!/usr/bin/env node
// **轮次编号唯一性判据** ✓。
//
// **为什么需要它** ✗：`docs/design/implementation-notes.md` 是**只追加**日志 ✓，
// 而多个子代理/多次合并都会各自"挑一个下一个号码" ⇒ **重号会悄悄积累** ✗
// （历史实测：一次合并后出现两组「第 1211 轮」✓）。
// 重号本身不破坏构建 ✓，但它让"第 N 轮"**无法唯一定位** ✗ ⇒ 引用它就变成歧义 ✓。
//
// **判据** ✓：每个 `## 第 N 轮` 的 N **必须唯一** ✓。
// **能红** ✓：把任意一条的编号改成已存在的编号 ⇒ 立刻红 ✓。
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const file = join(root, "docs/design/implementation-notes.md");
const text = readFileSync(file, "utf8");

const seen = new Map();
const dups = [];
for (const m of text.matchAll(/^## 第 (\d+) 轮/gm)) {
  const n = m[1];
  if (seen.has(n)) dups.push(n); else seen.set(n, true);
}

const total = seen.size + dups.length;
if (dups.length > 0) {
  console.error(`❌ 轮次编号重复 ⇒ 判据红：共 ${total} 条，重复编号 ${dups.length} 个`);
  for (const d of [...new Set(dups)].sort((a, b) => a - b)) console.error(`   第 ${d} 轮 出现多次`);
  console.error("   处理：给重复出现的那一条改成未使用的编号（保持追加语义，不改历史正文）。");
  process.exit(1);
}
console.log(`✓ 轮次编号唯一：共 ${total} 条，无重复 ✓`);
