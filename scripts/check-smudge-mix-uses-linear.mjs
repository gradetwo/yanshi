#!/usr/bin/env node
// **涂抹混合必须走**共享的线性函数**（第 1309 轮；Phase 1 的接线判据）
//
// **⇒ 为什么单测不够** ✗：
//   `phase1_smudge_mix` 测的是**函数本身**（`mix_smudge_linear` 的数学）。
//   ⇒ 而**它不检查 `stroke.rs` 有没有调它** ⇒ 若有人把 `stroke.rs` 改回**内联 sRGB 公式**，
//     **单测照样全绿**（函数还在，只是没人用了）✗
//   ⇒ 所以「接线」必须单独有一条判据 ✓
//
// **⇒ 断什么**（四条）：
//   ① `vendor/hokusai-core/src/color.rs` 定义 `pub fn mix_smudge_linear`
//   ② 那个函数体内**同时**出现 `srgb_to_linear` 与 `linear_to_srgb`（局部往返）
//   ③ `vendor/hokusai-core/src/stroke.rs` **调用** `mix_smudge_linear`
//   ④ `stroke.rs` 里的**旧内联公式已消失**（`smudge_amt * state.smudge_ra + col_factor * base.r`）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**（文件或锚点找不到 ⇒ 判据自己坏了）
import { readFileSync, existsSync } from "node:fs";

const COLOR = "vendor/hokusai-core/src/color.rs";
const STROKE = "vendor/hokusai-core/src/stroke.rs";
for (const f of [COLOR, STROKE]) {
  if (!existsSync(f)) {
    console.error(`  ⊘ 不可判：找不到 ${f}`);
    process.exit(3);
  }
}

const fail = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fail.push(m); console.log(`  ✗ ${m}`); };

const codeLines = (file) => {
  const out = [];
  let inTest = false;
  for (const [i, l] of readFileSync(file, "utf8").split("\n").entries()) {
    if (/^\s*#\[cfg\(test\)\]/.test(l)) inTest = true;
    if (inTest) {
      if (/^}\s*$/.test(l)) inTest = false;
      continue;
    }
    const t = l.trim();
    if (t.startsWith("//") || t.startsWith("///") || t.startsWith("//!")) continue;
    out.push([i + 1, l]);
  }
  return out;
};

// ---- ① + ② color.rs ----
const colorCode = codeLines(COLOR);
const defIdx = colorCode.findIndex(([, l]) => /pub\s+fn\s+mix_smudge_linear\s*\(/.test(l));
if (defIdx < 0) {
  console.error("  ⊘ 不可判：color.rs 里没有 `pub fn mix_smudge_linear` ⇒ 判据的锚点失效");
  process.exit(3);
}
ok(`① color.rs 定义了 mix_smudge_linear（:${colorCode[defIdx][0]}）`);

// 函数体：从定义行到该函数闭合（按花括号深度）
let depth = 0;
let started = false;
const body = [];
for (let k = defIdx; k < colorCode.length; k += 1) {
  const [, l] = colorCode[k];
  depth += (l.match(/{/g) ?? []).length - (l.match(/}/g) ?? []).length;
  if (!started && depth > 0) started = true;
  body.push(l);
  if (started && depth === 0) break;
}
const bodyHasTo = body.some((l) => /srgb_to_linear\s*\(/.test(l));
const bodyHasBack = body.some((l) => /linear_to_srgb\s*\(/.test(l));
if (bodyHasTo && bodyHasBack) {
  ok("② 它的函数体里同时有 srgb_to_linear 与 linear_to_srgb（局部往返）");
} else {
  bad(`② 函数体必须同时有 srgb_to_linear 与 linear_to_srgb ⇒ 实测 去=${bodyHasTo} 回=${bodyHasBack}`);
}

// ---- ③ + ④ stroke.rs ----
const strokeCode = codeLines(STROKE);
const calls = strokeCode.filter(([, l]) => /mix_smudge_linear\s*\(/.test(l));
if (calls.length > 0) {
  ok(`③ stroke.rs 调用了 mix_smudge_linear（${calls.length} 处，首处 :${calls[0][0]}）`);
} else {
  bad("③ stroke.rs 必须调用 mix_smudge_linear（否则混合还是内联的 sRGB 公式）");
}

const inline = strokeCode.filter(
  ([, l]) => /smudge_amt\s*\*\s*state\.smudge_r/.test(l) && /col_factor\s*\*\s*base\.r/.test(l),
);
if (inline.length === 0) {
  ok("④ stroke.rs 里的旧内联公式已消失（smudge_amt*smudge_r + col_factor*base.r）");
} else {
  bad(`④ stroke.rs 里仍有旧内联公式 ${inline.length} 处（首处 :${inline[0][0]}）⇒ 混合没走线性函数`);
}

// ---- ⑤ 服务端路径（`smudge_dab`）也必须是线性混合 ----
//   **为什么单列**：服务端**不走** hokusai 的 `stroke.rs`（它有自己 `tools.rs:12503` 的 `stamp_stroke`）
//     ⇒ 所以上面 ①–④ 只守住 **wasm** 那条 ⇒ 服务端要单独守 ✓
const DAB = "crates/yanshi-render/src/brush.rs";
if (!existsSync(DAB)) {
  console.error(`  ⊘ 不可判：找不到 ${DAB}`);
  process.exit(3);
}
const dabCode = codeLines(DAB);
const dabIdx = dabCode.findIndex(([, l]) => /pub\s+fn\s+smudge_dab\s*\(/.test(l));
if (dabIdx < 0) {
  console.error("  ⊘ 不可判：brush.rs 里没有 `pub fn smudge_dab`");
  process.exit(3);
}
let d2 = 0;
let started2 = false;
const dabBody = [];
for (let k = dabIdx; k < dabCode.length; k += 1) {
  const [, l] = dabCode[k];
  d2 += (l.match(/{/g) ?? []).length - (l.match(/}/g) ?? []).length;
  if (!started2 && d2 > 0) started2 = true;
  dabBody.push(l);
  if (started2 && d2 === 0) break;
}
const dabHasTo = dabBody.some((l) =>
  // **P-4（2026-10-28）之后解码走 fix15 查表**：`decode_fix15_with_table` 的表项**由
  //   `srgb_to_linear` 逐项生成**、Rust 测试 `fix15_decode_table_is_bitwise_exact` 钉了
  //   **逐位相同** ⇒ 「线性空间混合」的语义没变 ⇒ 两种形态都算在线性侧解码 ✓
  //   （变异核对：若有人把解码整个删掉、直接对 sRGB u16 插值 ⇒ 两个模式都不中 ⇒ 红 ✓）
  /srgb_to_linear\s*\(/.test(l) ||
  /decode_fix15_with_table\s*\(/.test(l) ||
  /srgb_decode_fix15_table\s*\(/.test(l));
const dabHasBack = dabBody.some((l) => /linear_to_srgb\s*\(/.test(l));
if (dabHasTo && dabHasBack) {
  ok(`⑤ 服务端 smudge_dab 的颜色混合也是线性的（:${dabCode[dabIdx][0]}，去/回都在）`);
} else {
  bad(`⑤ 服务端 smudge_dab 必须在线性空间混合 ⇒ 实测 去=${dabHasTo} 回=${dabHasBack}`);
}

if (fail.length > 0) {
  console.log(`\n  ✗ ${fail.length} 处不符（涂抹混合必须走共享的线性函数）`);
  process.exit(1);
}
console.log("\n  ✓ 涂抹混合走共享的线性函数，且旧内联公式已移除");
