#!/usr/bin/env node
// **`vendor/` 的平台相关浮点调用**只许减少，不许增加**（棘轮；第 1338 轮建立）
//
// **⇒ 背景**（第 1337 轮实测）：
//   `tool-kernel-determinism` 只扫 `crates/` ⇒ 而 `vendor/hokusai-core/src` 里有
//   **55 处**平台相关的浮点调用（`.exp` 21／`.powf` 10／`.sin` 7／`.hypot` 7／`.atan2` 5／`.cos` 3／`.ln` 2）
//   ⇒ 所以「内核同源数学」这条承诺**只覆盖了一半** ✗
//
// **⇒ 为什么是棘轮而不是要求 0**：
//   那 55 处全是**上游既有**的 ⇒ 一次全改会动 `stroke.rs` 的每个 dab（热路径）⇒ 风险 > 收益
//   ⇒ 而**完全不管**会让新增的平台相关调用**无人拦** ✗
//   ⇒ 棘轮：既有在案、新增必红、减少可收敛 ✓
//
// **⇒ 断什么**：
//   ① 每个文件的当前处数 **≤ 基线**（超了 ⇒ 红，并说清多了几处、哪几个函数）
//   ② 基线里列的文件**必须存在**（消失 ⇒ 红：基线该更新）
//   ③ 当前有没有**不在基线里**的文件（新增文件带平台相关调用 ⇒ 红）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync, existsSync, readdirSync } from "node:fs";

const VENDOR = "vendor/hokusai-core/src";
const BASE = "scripts/vendor-math-baseline.txt";

// **⇒ 平台相关的函数**（IEEE 允许的 `sqrt`／可展开的 `powi` **不在**此列 ✓）
const BAD = [
  "sin", "cos", "tan", "asin", "acos", "atan", "atan2",
  "exp", "ln", "log", "log2", "log10", "powf", "hypot", "cbrt",
];

if (!existsSync(VENDOR)) {
  console.error(`  ⊘ 不可判：找不到 ${VENDOR}`);
  process.exit(3);
}
if (!existsSync(BASE)) {
  console.error(`  ⊘ 不可判：找不到基线 ${BASE}`);
  process.exit(3);
}

const files = readdirSync(VENDOR).filter((f) => f.endsWith(".rs"));
if (files.length === 0) {
  console.error("  ⊘ 不可判：vendor 里没有 .rs 文件（路径改名？）");
  process.exit(3);
}

// **⇒ 基线解析**
const base = new Map();
for (const line of readFileSync(BASE, "utf8").split("\n")) {
  const t = line.trim();
  if (!t || t.startsWith("#")) continue;
  const m = t.match(/^([a-z0-9_]+\.rs)\s+(\d+)$/);
  if (!m) {
    console.error(`  ⊘ 不可判：基线行格式不对 ⇒ ${t}`);
    process.exit(3);
  }
  base.set(m[1], Number(m[2]));
}

// **⇒ 计数**（按函数分开统计，报错时能说清是哪个函数多了）
const count = new Map();
for (const f of files) {
  const text = readFileSync(`${VENDOR}/${f}`, "utf8");
  const per = new Map();
  for (const fn of BAD) {
    // 用带断言的方式：`\.fn\s*\(`
    const re = new RegExp(`\\.${fn}\\s*\\(`, "g");
    const n = (text.match(re) ?? []).length;
    if (n > 0) per.set(fn, n);
  }
  const total = [...per.values()].reduce((a, b) => a + b, 0);
  if (total > 0) count.set(f, { total, per });
}

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const baseTotal = [...base.values()].reduce((a, b) => a + b, 0);
const curTotal = [...count.values()].reduce((a, b) => a + b.total, 0);
console.log(`  扫描 ${VENDOR} ⇒ ${files.length} 个文件｜基线 ${baseTotal} 处｜当前 ${curTotal} 处`);

// ① 每个文件的当前 ≤ 基线
let increased = 0;
for (const [f, { total, per }] of count) {
  const b = base.get(f) ?? 0;
  if (total > b) {
    const detail = [...per.entries()].map(([k, v]) => `.${k}×${v}`).join(" ");
    bad(`① ${f}：基线 ${b} 处，当前 ${total} 处（多 ${total - b} 处）⇒ ${detail}`);
    increased += 1;
  }
}
if (increased === 0) ok(`① 没有任何文件超过基线（${count.size} 个文件有平台相关调用）`);

// ② 基线里的文件必须存在
const missing = [...base.keys()].filter((f) => !existsSync(`${VENDOR}/${f}`));
if (missing.length === 0) ok("② 基线里的文件都还在");
else bad(`② 基线里的文件不见了 ⇒ ${missing.join("、")}（基线该更新）`);

// ③ 不许有「不在基线里」的新文件带平台相关调用
const unlisted = [...count.keys()].filter((f) => !base.has(f));
if (unlisted.length === 0) ok("③ 没有未登记的文件带平台相关调用");
else bad(`③ 这些文件不在基线里却带平台相关调用 ⇒ ${unlisted.join("、")}`);

// 减少 ⇒ 提示可调低（不是失败）
const reduced = [...count.entries()].filter(([f, { total }]) => total < (base.get(f) ?? 0));
if (reduced.length > 0) {
  console.log("  ○ 低于基线（可调低基线，单调收敛）：");
  for (const [f, { total }] of reduced) console.log(`      ○ ${f}：${base.get(f)} ⇒ ${total}`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（vendor 的平台相关调用只许减少）`);
  process.exit(1);
}
console.log(`\n  ✓ vendor 棘轮守住（${curTotal} 处，未超过基线 ${baseTotal} 处）`);
