#!/usr/bin/env node
// **涂抹混合必须走**共享的线性函数**（第 1309 轮；Phase 1 的接线判据）
//
// **⇒ 为什么单测不够** ✗：
//   `phase1_smudge_mix` 测的是**函数本身**（`mix_smudge_linear` 的数学）。
//   ⇒ 而**它不检查 `stroke.rs` 有没有调它** ⇒ 若有人把 `stroke.rs` 改回**内联 sRGB 公式**，
//     **单测照样全绿**（函数还在，只是没人用了）✗
//   ⇒ 所以「接线」必须单独有一条判据 ✓
//
// **⇒ 断什么**（五条；①–④＝wasm 侧，⑤＝服务端侧）：
//   ① `vendor/hokusai-core/src/color.rs` 定义 `pub fn mix_smudge_linear`
//   ② 那个函数体内**同时**出现 `srgb_to_linear` 与 `linear_to_srgb`（局部往返）
//   ③ `vendor/hokusai-core/src/stroke.rs` **调用** `mix_smudge_linear`
//   ④ `stroke.rs` 里的**旧内联公式已消失**（`smudge_amt * state.smudge_ra + col_factor * base.r`）
//
//   ⑤（**D 期改口径**，见下方 ⑤ 段）服务端 `smudge_dab` 在**预乘空间做四通道凸组合**
//     ＋ **不许出现 gamma 往返**（`srgb_to_linear`／`linear_to_srgb`／fix15 解码查表）
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

// ---- ⑤ 服务端路径（`smudge_dab`）：**D 期起改断「预乘空间凸组合」**（2026-10-29）----
//   **为什么改口径**（不是放宽，是跟着权威实现走；内核②「同一事实只写一处」的判据侧表现）：
//   · 旧 ⑤ 断「函数体里出现 gamma 往返（`srgb_to_linear`／`linear_to_srgb`／fix15 查表）」——
//     那是「把 tile 值当**直通 sRGB** 解码后线性混合」时代的接线守卫。
//   · 而 D 期实测证明那个前提**本身是错的**：tile 存的是 fix15 **预乘**值 ⇒
//     解码混合再编码写回**不乘回 alpha** ⇒ 读回（反预乘除 alpha）后直通色被放大
//     （墨 60→84、复利到白 = 纸色底淡色扇弧；同色异 alpha 64→92 = 淡隙条纹）。
//   · 新实现 = **预乘空间的四通道凸组合**（`d*(1-a)+s*a` 四通道同式、无 gamma 变换）⇒
//     结果恒在两输入之间、不可能超调；不透明像素上预乘=直通 ⇒ 与旧实现两端逐位一致。
//   · 行为面已有真守卫：brush.rs 两条新回归（透明源保色／同色异 alpha 保色，旧码双红）＋
//     `tool-smudge-deposit`／`tool-smudge-drags-color`。**本条 ⑤ 守的是「别把 gamma 往返改回来」**
//     ——单测抓不到「函数还在、没人调」那一类，文本接线判据抓得到 ✓
//   ⇒ 断两件事：**(a)** 凸组合接线在（每通道 `f32::from(d) * (1.0 - amount) + f32::from(s) * amount`）；
//     **(b)** 函数体内**没有** gamma 往返调用（出现即红 ⇒ 变异：把 mix 换回 decode/encode ⇒ 必红 ✓）。
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
// (a) 预乘凸组合：四通道同式的那一次混合（锚 = D 期 `let mix = |d: u16, s: u16|` 的式子）
const convex = dabBody.filter((l) =>
  /f32::from\(\s*d\s*\)\s*\*\s*\(\s*1\.0\s*-\s*amount\s*\)\s*\+\s*f32::from\(\s*s\s*\)\s*\*\s*amount/.test(l),
).length;
if (convex === 0) {
  console.error("  ⊘ 不可判：smudge_dab 里找不到预乘凸组合的锚 ⇒ 实现又变了，判据口径要跟着权威改");
  process.exit(3);
}
ok(`⑤a 服务端 smudge_dab 在预乘空间做凸组合（锚 ${convex} 处，函数起始 :${dabCode[dabIdx][0]}）`);
// (b) gamma 往返必须缺席（四种形态都算「改回来了」）
const gamma = dabBody.filter((l) =>
  /srgb_to_linear\s*\(/.test(l) ||
  /linear_to_srgb\s*\(/.test(l) ||
  /decode_fix15_with_table\s*\(/.test(l) ||
  /srgb_decode_fix15_table\s*\(/.test(l),
);
if (gamma.length === 0) {
  ok("⑤b smudge_dab 函数体内没有 gamma 往返（srgb_to_linear／linear_to_srgb／fix15 查表全部缺席）");
} else {
  bad(`⑤b smudge_dab 里又出现 gamma 往返 ${gamma.length} 处 ⇒ D 期前提：tile 是预乘值，解码混合会放大直通色（墨 60→84 复利到白）`);
}

if (fail.length > 0) {
  console.log(`\n  ✗ ${fail.length} 处不符（wasm 侧必须走共享线性函数；服务端必须走预乘凸组合）`);
  process.exit(1);
}
console.log("\n  ✓ 涂抹混合接线与当前口径一致（wasm=线性共享函数 ①–④｜服务端=预乘凸组合 ⑤）");
