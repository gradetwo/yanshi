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
const FILES = ["viewer-app.js", "viewer.css", "i18n.js", "service-worker.js", "api-local.js", "store.js"];

let failed = 0;
console.log("  ── 源头 vs web（逐字节）──");
for (const f of FILES) {
  const a = join(SRC, f);
  const b = join(DST, f);
  if (!existsSync(a)) { console.log(`    ⚠ ${f}：源头没有（跳过）`); continue; }
  if (!existsSync(b)) { console.log(`    ✗ ${f}：web 侧缺失`); failed += 1; continue; }
  // **★★★ 收口（乙）：比较前把**缓存戳**归一化 ✗ ★★★**（**第 600 轮 ✓；目标第 12 条 ✓）
  //   **∴ 为什么必须归一化 ✗**：
  //     **∴ ①** `web/` 的那两个文件**带**缓存戳**（`yanshi-shell-<数字>` ✓）
  //       ＋ **∴ 而** `crates/yanshi-http/assets/` 的**源头**不带** ✓
  //         （**∴ 因为**戳**由内容算出**✗ ⇒ **∴ 源头带戳会**自指** ✓）
  //           ⇒ **∴ 于是**：**两侧**永远差 1 字节 ⇒ **∴ 判据**永远红** ✓
  //     **∴ ② 而** `pwa-sync-viewer.mjs` **自己**算哈希前就归一化**（**第 493 轮 ✓）
  //       ⇒ **∴ 所以**：**本判据用同一规则**才**与设计一致** ✓ ★
  //   **∴ 保留原判据的力量 ✗**：**先比原始字节**✗
  //     ＋ **∴ 若**不等 ⇒ **∴ 再比**归一化后**的文本** ✓
  //       ⇒ **∴ 于是**：**真分叉（**内容不同 ✓）**仍然红** ✓
  //         ＋ **∴ 只有**戳不同 ⇒ **✓ 并注明** ✓ ★
  const rawA = readFileSync(a);
  const rawB = readFileSync(b);
  // **∴ 必须同时覆盖**两种形态 ✗**（**第 600 轮实测 ✓）：
  //   **∴ 源头用**占位符 `yanshi-shell-__BUILD_ID__`**✗
  //     ＋ **∴ 而** `web/` 用**真实数字戳**`yanshi-shell-<数字>`** ✓
  //       ⇒ **∴ 所以**：**只匹配数字 ⇒ **源头那侧替不掉**✗
  //         ⇒ **∴ 于是**：**归一化后**仍不同** ✓ ★
  const stampRe = /yanshi-shell-(?:__BUILD_ID__|[0-9]+)/g;
  const normA = rawA.toString("utf8").replace(stampRe, "yanshi-shell-STAMP");
  const normB = rawB.toString("utf8").replace(stampRe, "yanshi-shell-STAMP");
  const sameRaw = rawA.equals(rawB);
  const same = sameRaw || normA === normB;
  const note = sameRaw ? "" : same ? "（**仅缓存戳不同 ✓**）" : "";
  if (!same) failed += 1;
  console.log(`    ${same ? "✓" : "✗"} ${f}（源头 ${statSync(a).size} B｜web ${statSync(b).size} B）${note}`);
}
console.log("");
if (failed === 0) {
  console.log("  结论：✓ 所有可同步的前端文件两边逐字节一致");
  process.exit(0);
}
console.error(`  结论：✗ ${failed} 个文件不一致 ⇒ **∴ 改源头后**必须**跑 npm run pwa:sync** ✓`);
process.exit(1);
