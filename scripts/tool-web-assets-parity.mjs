#!/usr/bin/env node
// **★ 源头与 web 的同名前端文件必须**逐字节一致** ✗ ★**（第 240 轮 ✓；**用户第 593 轮「单一真源」 ✓）。
//
// **∴ 为什么 ✗**（**实测第 239 轮 ✓）**✗**：
//   **∴ `api-local.js` **在 `stampSource` 里**✗（**戳跟着它变 ✓）
//     ⇒ **∴ 而**它**不在 `FILES` 里** ✓
//       ⇒ **∴ 于是**：**web/ 的它**永远不被自动覆盖** ✓
//         ⇒ **∴ 所以**：**源头与 web **长期不同** ✓（**实测 diff 12 行 ✓）**
//           ⇒ **★ 那**正是**「**手工维护**」的来源** ✓ ★**** ✓✓
//   **∴ 修法 ✗**：**把可同步项加进 `FILES`**✗ ⇒ **∴ 于是**源头**唯一真源** ✓
// **∴ 本判据 ✗**：**逐个比对**源头与 web **的同名文件** ✓
//   ⇒ **∴ 于是**：**将来**谁**改了源头忘了 web**✗ ⇒ **∴ 立刻红** ✓**** ✓✓
//
// **∴ web 独有项 ✗**：**`brush-local.js` **源头没有**✗（**实测 ✓）
//   ⇒ **∴ 它**只能手工维护** ✓ ⇒ **∴ 本判据**只查**两边都有的** ✓
//
// **∴ 变异点 ✗**：**改源头的一个可同步文件**✗ ⇒ **∴ 判据必红** ✓
//
// 用法：node scripts/tool-web-assets-parity.mjs

import { readFileSync, existsSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = "crates/yanshi-http/assets";
const DST = "web";
// **∴ 要与 `pwa-sync-viewer.mjs` 的 `FILES` **保持一致** ✗ ★**
const FILES = ["viewer-app.js", "viewer.css", "service-worker.js", "api-local.js", "store.js"];

let failed = 0;
console.log("  ── 源头 vs web（逐字节）──");
for (const f of FILES) {
  const a = join(SRC, f);
  const b = join(DST, f);
  if (!existsSync(a)) { console.log(`    ⚠ ${f}：源头没有（跳过）`); continue; }
  if (!existsSync(b)) { console.log(`    ✗ ${f}：web 侧缺失`); failed += 1; continue; }
  const same = readFileSync(a).equals(readFileSync(b));
  console.log(`    ${same ? "✓" : "✗"} ${f}（源头 ${statSync(a).size} B｜web ${statSync(b).size} B）`);
  if (!same) failed += 1;
}
console.log("");
if (failed === 0) {
  console.log("  结论：✓ 所有可同步的前端文件两边逐字节一致");
  process.exit(0);
}
console.error(`  结论：✗ ${failed} 个文件不一致 ⇒ **∴ 改源头后**必须**跑 npm run pwa:sync** ✓`);
process.exit(1);
