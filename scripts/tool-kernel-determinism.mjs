#!/usr/bin/env node
// **内核的数学必须两端同源**（第 809 轮）：`crates/yanshi-render` 与 `crates/yanshi-core` 是
// **服务端与浏览器共用**的内核 ✓ ⇒ 里面**不许**出现"各平台各自实现"的浮点函数 ✗。
//
// **为什么需要它** ✗（第 801 轮的微实验 ✓）：`std` 的 `sin/cos/exp/powf/ln` 在 native（glibc ✓）
// 与 wasm32（Rust 自带 ✓）之间**差最后一位** ✗ —— 实测 834 个输入里 sin 9 处、exp 79 处不同，
// **而 `sqrt` 零差异** ✓（它是唯一由 IEEE-754 要求**正确舍入**的 ✓）。
// 一位之差会被笔刷动力学放大 ⇒ **两种渲染模式对约一半笔刷给出不同画面** ✗（第 800 轮实测 ✓）。
// ⇒ 修法是换成 `libm`（纯 Rust ✓ ⇒ 同一份源码编两次 ⇒ 逐位相同 ✓），本条判据守住这个决定 ✓。
//
// **判据（能红 ✓）**：
//   ① 内核源码（**排除注释行** ✓）里不得出现 `.sin(` / `.powf(` / `.hypot(` / `.powi(` …
//      这类"平台各自实现"的浮点方法调用 ✓ ⇒ 出现即红 ✓，并**点名文件与行** ✓；
//   ② **允许**：`libm::…` ✓、`.sqrt(` ✓（IEEE 要求正确舍入 ✓，实测两端零差异 ✓）、
//      `.to_radians()` / `.to_degrees()` ✓（纯乘法 ✓，精确 ✓）、`.abs/.floor/.ceil/.round/.trunc` ✓（精确 ✓）；
//   ③ **扫描不到任何文件就报错** ✓（不许静默通过 ✗ —— 目录改名不能让这条判据变成空话 ✓）；
//   ④ **打印覆盖面** ✓（扫了几个文件、几处 `libm` 调用 ✓）。
//
// 用法：node scripts/tool-kernel-determinism.mjs   （不需要服务端 ✓）
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

// ⚠️ **范围必须包含 wasm 门面层** ✗（第 821 轮实测 ✓）：原先只扫 render/core ✓ ⇒
// `crates/yanshi-wasm/src/brush.rs:158` 的 `.powi(2)` **整整漏了一轮** ✗ ⇒
// 而它正是"两种模式画面不同"的一项成因 ✓（差 1 ulp ⇒ `ceil` 在边界上差 1 ⇒ 步数差 1 ⇒
// 多或少一个 dab ⇒ 一块 tile 大小的差异 ✓）。**浏览器真正加载的那份内核就在这里** ✓。
const ROOTS = ["crates/yanshi-render/src", "crates/yanshi-core/src", "crates/yanshi-wasm/src"];
// **"各平台各自实现"的浮点函数** ✗（`sqrt` 不在其中 ✓：IEEE 要求它正确舍入 ✓）
const FORBIDDEN = [
  "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh",
  "exp", "exp2", "ln", "log2", "log10", "powf", "powi", "hypot", "cbrt", "mul_add",
];
const forbiddenRe = new RegExp("\\.(" + FORBIDDEN.join("|") + ")\\s*\\(");

let files = 0;
let libmCalls = 0;
let sqrtCalls = 0;
const offences = [];

for (const root of ROOTS) {
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch {
    console.error(`❌ 扫不到目录 ${root} ⇒ 判据无法运行（不是通过 ✗）`);
    process.exit(1);
  }
  for (const entry of entries) {
    if (!entry.isFile() || !entry.name.endsWith(".rs")) continue;
    files += 1;
    const path = join(root, entry.name);
    readFileSync(path, "utf8").split("\n").forEach((text, index) => {
      const trimmed = text.trim();
      if (trimmed.startsWith("//")) return; // 注释里提到函数名不算违规 ✓
      libmCalls += (text.match(/libm::/g) || []).length;
      sqrtCalls += (text.match(/\.sqrt\s*\(/g) || []).length;
      const m = text.match(forbiddenRe);
      if (m) offences.push({ path, line: index + 1, call: m[0], text: trimmed });
    });
  }
}

if (files === 0) {
  console.error("❌ 一个 .rs 也没扫到 ⇒ 判据变成空话（不是通过 ✗）");
  process.exit(1);
}

console.log(`  扫描：${ROOTS.join(" + ")} ⇒ ${files} 个文件`);
console.log(`  同源数学调用：libm:: ${libmCalls} 处 ✓｜允许的 .sqrt( ${sqrtCalls} 处 ✓（IEEE 要求正确舍入 ✓）`);

if (offences.length > 0) {
  console.error(`  ✗ 内核里出现了 ${offences.length} 处"各平台各自实现"的浮点调用 ⇒ 两种渲染模式可能给出不同画面：`);
  for (const o of offences.slice(0, 12)) {
    console.error(`     - ${o.path}:${o.line} ⇒ ${o.call}｜${o.text.slice(0, 70)}`);
  }
  console.error("  ⇒ 修法：换成 `libm::…`（纯 Rust ✓ ⇒ 同一份源码编两次 ⇒ 逐位相同 ✓）；");
  console.error("     `.powi(2)` 这类**能消掉的**就直接写成乘法 ✓（乘法是 IEEE 精确规定的 ✓）。");
  process.exit(1);
}
console.log("  ✓ 内核里的数学全部同源（或属于 IEEE 要求精确的那些运算）⇒ 两端结果逐位相同由构造保证");
