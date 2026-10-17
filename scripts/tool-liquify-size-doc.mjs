#!/usr/bin/env node
// **`liquify_push` 的 `size` 说明必须说清"直径"与"没有 radius"** ✓
//   （第 1000+ 轮 ✓；YANSHI_ISSUES 的 §14）
//
// **∴ 为什么值得一条判据 ✓**：报告说"没有 radius"✗ —— **∴ 但缺参数**会报错 ✓（**∴ 报错列了可用参数 ✓**）；
//   真会**静默出错**的是 `size` 是**直径** ✗ —— **∴ 把它当半径用 ⇒ 实际半径只有一半 ✗ ⇒ **∴ 差 2 倍且不报错**✗**
//   ⇒ **∴ 所以这句话是**唯一**能防住那个 2 倍错的提示 ✓** ✓
//
// **∴ 判据怎么判 ✓**：读源码 ✓（与 `tool-ci-parity-budget.mjs` 同型 ✓），
//   在 `name: "liquify_push"` 的块里找 `param!("size"` 那一行 ✓
//   ⇒ **∴ 必须同时含「直径」与「radius」**✗（**∴ 少了任一个 ⇒ 读者就不知道"直径≠半径"✓**）✓
import { readFileSync } from "node:fs";

const src = readFileSync(new URL("../crates/yanshi-server/src/tools.rs", import.meta.url), "utf8");
const lines = src.split("\n");

const start = lines.findIndex((l) => l.includes('name: "liquify_push"'));
if (start < 0) { console.log("  ✗ 源码里找不到 name: \"liquify_push\""); process.exit(1); }

let sizeLine = null;
for (let i = start; i < Math.min(start + 200, lines.length); i += 1) {
  if (lines[i].includes('param!("size"')) { sizeLine = lines[i]; break; }
  if (i > start && lines[i].includes('name: "')) break;   // 已经走到下一个工具了
}
if (!sizeLine) {
  console.log("  ✗ liquify_push 的块里找不到 param!(\"size\")  ⇒ 判据跑不了（不是产品问题）");
  process.exit(3);
}
console.log(`  找到 size 的说明 ✓：${sizeLine.trim().slice(0, 120)}`);

const hasDiameter = /直径/.test(sizeLine);
const hasRadius = /radius/i.test(sizeLine);
console.log(`  ${hasDiameter ? "✓" : "✗"} 含「直径」（否则读者会当成半径）`);
console.log(`  ${hasRadius ? "✓" : "✗"} 含「radius」（否则读者不知道这个名字不存在／要 ×2）`);
if (!hasDiameter || !hasRadius) {
  console.log("  ✗ liquify_push 的 size 说明没有说清「直径 vs 半径」——这正是那个 2 倍静默错的入口");
  process.exit(1);
}
console.log("  ✓ liquify_push 的 size 说明同时说清了直径与 radius");
