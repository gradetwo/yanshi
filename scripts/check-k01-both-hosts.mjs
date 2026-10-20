#!/usr/bin/env node
// **两个宿主都必须算 `stroke_total_length`，且必须调**同一个**函数** ✓
// （第三轮复审 **K-01**，P0：只有 native 算 ⇒ wasm 恒 0 ⇒ 双宿主输出不一致）
//
// **∴ 这条判据守什么**（3 条断言，各抓一种回归）：
//   ① `crates/yanshi-server/src/tools.rs` 的 `prime_stroke_total_length` 必须调
//      `yanshi_core::stroke_length::total_length`（不许自己再写一份算法 ⇒ 不许漂移）
//   ② `crates/yanshi-wasm/src/brush.rs` 的 `stamp` 里必须有
//      `stroke_total_length` 赋值（否则 wasm 恒 0 ⇒ pressure_shape 失效）
//   ③ 那个赋值也必须来自**同一个**函数（`yanshi_core::stroke_length::total_length`）
//
// **∴ 为什么是静态判据**：要端到端验「native 与 wasm 渲染逐像素相同」得有 wasm 运行时
//   （本仓的 wasm 判据要 wasm-pack + 浏览器）。而这条缺陷的**形状**是「一个宿主忘了算」——
//   静态判据能抓到它，且跑得极快（毫秒级）✓
//
// **∴ 变异**：把 wasm 侧那行删掉 ⇒ ② 必红 ✓
import { readFileSync } from "node:fs";

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

const SHARED = "yanshi_core::stroke_length::total_length";

// ① 服务端：prime_stroke_total_length 必须调共享函数
let server = "";
try {
  server = readFileSync("crates/yanshi-server/src/tools.rs", "utf8");
} catch {
  console.error("  ⊘ 不可判：读不到 crates/yanshi-server/src/tools.rs");
  process.exit(3);
}
const primeStart = server.indexOf("fn prime_stroke_total_length");
if (primeStart < 0) {
  console.error("  ⊘ 不可判：tools.rs 里找不到 prime_stroke_total_length");
  process.exit(3);
}
const primeBody = server.slice(primeStart, primeStart + 1200);
check(primeBody.includes(SHARED), `① 服务端的 prime_stroke_total_length 必须调 ${SHARED}`);

// ② wasm：stamp 里必须有 stroke_total_length 赋值
let wasm = "";
try {
  wasm = readFileSync("crates/yanshi-wasm/src/brush.rs", "utf8");
} catch {
  console.error("  ⊘ 不可判：读不到 crates/yanshi-wasm/src/brush.rs");
  process.exit(3);
}
const stampStart = wasm.indexOf("pub(crate) fn stamp(");
if (stampStart < 0) {
  console.error("  ⊘ 不可判：brush.rs 里找不到 stamp");
  process.exit(3);
}
// 取到函数体结束（下一个顶层 `}` 前的 3000 字符足够覆盖开头那段）
// **★ 必须剥离注释再匹配**（我这一轮就是这么错的：变异把赋值注释掉，
//   而注释里仍留着 `state.stroke_total_length =` 和函数名 ⇒ 判据照样绿 ✗）
//   ⇒ 这是「注释被当成代码」的**第 2 次**（上一次在 check-putatom-single-transaction）
const stripComments = (text) =>
  text
    .split("\n")
    .map((line) => {
      const i = line.indexOf("//");
      return i >= 0 ? line.slice(0, i) : line;
    })
    .join("\n");
const stampBody = stripComments(wasm.slice(stampStart, stampStart + 3000));
check(
  /state\.stroke_total_length\s*=/.test(stampBody),
  `② wasm 的 stamp 里必须有 state.stroke_total_length 赋值（否则压力曲线失效）`,
);

// ③ 两宿主必须用**同一个**函数
check(stampBody.includes(SHARED), `③ wasm 的赋值也必须来自 ${SHARED}（不许各写一份）`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（K-01：两宿主的总弧长）`);
  process.exit(1);
}
console.log("\n  ✓ 两个宿主都算 stroke_total_length，且调同一个共享函数");
