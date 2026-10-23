#!/usr/bin/env node
// **`putAtom` 必须用**一个**事务包住「读 seq + 写 atoms + 写 docs」** ✓
// （第三轮复审 **D-01**，P0：三段独立事务 ⇒ 并发时主键相撞 ⇒ 笔触静默消失）
//
// **∴ 为什么是静态判据**：这条缺陷是**并发竞态**，要复现得在真 IndexedDB 里
//   并发调两次 `putAtom` ⇒ 而本仓的 browser 判据需要 chromium 条件（本机不稳）。
//   ⇒ 所以这里断**结构**（它能抓到回归）：**putAtom 里 `db.transaction` 调用必须恰好 1 次**，
//     且它必须**同时**列出 `docs` 与 `atoms` 两个 store，且**必须** `await` 事务完成。
//
// **∴ 这四个断言各抓一种回归**：
//   ① 事务数 > 1  ⇒ 有人又把写拆开（D-01 原样）
//   ② 只列一个 store ⇒ 跨 store 不原子（等于没修）
//   ③ 不 await 完成 ⇒ 「发出去了」冒充「写进去了」
//   ④ `timedOut` 只写不读 ⇒ **死标志**（第 1440 轮审计：超时兜底设了旗子却没人看 ⇒
//      旧注释声称的 `txTimeout` 字段从未存在 ⇒ 静默降级第 5 形态）。**∴ 必须读它 ✗**
//
// **∴ 变异**：把 `await done;` 删掉 ⇒ ③ 必红 ✓；把 `if (timedOut)` 那行删掉 ⇒ ④ 必红 ✓
import { readFileSync } from "node:fs";

const src = readFileSync("web/store.js", "utf8");
// 抽出 putAtom 函数体（从 `export async function putAtom` 到下一个顶层 `export` 或文件尾）
const start = src.indexOf("export async function putAtom");
if (start < 0) {
  console.error("  ⊘ 不可判：web/store.js 里找不到 putAtom");
  process.exit(3);
}
const rest = src.slice(start + 10);
const nextExport = rest.indexOf("\nexport ");
const body = nextExport > 0 ? rest.slice(0, nextExport) : rest;
// **去掉注释再数**（否则注释里的示例会被当成代码 —— 我本轮就差点被它骗了）
const code = body
  .split("\n")
  .filter((line) => !/^\s*(\/\/|\*|\/\*)/.test(line))
  .join("\n");

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

const txCount = (code.match(/db\.transaction\(/g) || []).length;
check(txCount === 1, `① putAtom 里 db.transaction 调用恰好 1 次（实测 ${txCount} 次）`);

const hasBoth = /db\.transaction\(\s*\[\s*"docs"\s*,\s*"atoms"\s*\]/.test(code);
check(hasBoth, `② 那个事务必须同时列 ["docs","atoms"]（跨 store 才原子）`);

const awaitsDone = /await\s+done\b/.test(code);
check(awaitsDone, `③ 必须 await 事务完成（不能「发出去了」冒充「写进去了」）`);

const timedOutRead = /if\s*\(\s*timedOut\s*\)/.test(code);
check(timedOutRead, `④ 超时旗标 timedOut 必须被读（只设不读＝死旗标 ⇒ 超时不可见 ✗ 第 1440 轮）`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（putAtom 的原子性）`);
  process.exit(1);
}
console.log("\n  ✓ putAtom 用单事务包住读 seq + 写 atoms + 写 docs，并等待提交");
