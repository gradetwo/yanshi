#!/usr/bin/env node
// **判据：仓库根不得出现"64 位十六进制命名"的目录** ✗。
//
// **为什么** ✓：`scripts/browser-offline-export.mjs` 曾把 `process.argv[4]`（**本该是 token**）
// 当作 CDP 的下载目录 ⇒ 每次规范运行都会在**仓库根**建一个 64 位十六进制名的目录
// （实测删过 13 个 ✓）。这类垃圾会污染仓库根，也会让"根目录白名单"那条判据失去意义。
//
// **判据** ✓：仓库根下**不存在**名字匹配 `^[0-9a-f]{64}$` 的**目录**。
// **能红** ✓：手动 `mkdir $(printf 'a%.0s' {1..64})` ⇒ 立刻红 ✓。
import { readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const bad = [];
for (const name of readdirSync(root)) {
  if (!/^[0-9a-f]{64}$/.test(name)) continue;
  try { if (statSync(join(root, name)).isDirectory()) bad.push(name); } catch (_) { /* 竞态：忽略 */ }
}
if (bad.length > 0) {
  console.error(`❌ 仓库根出现 64 位十六进制命名目录 ⇒ 判据红：${bad.length} 个`);
  for (const b of bad.slice(0, 5)) console.error(`   ${b}`);
  console.error("   处理：删掉它们；并检查是哪条判据把 token 当成了下载目录。");
  process.exit(1);
}
console.log("✓ 仓库根没有 64 位十六进制命名目录 ✓");
