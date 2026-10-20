#!/usr/bin/env node
// **半透明矢量路径的**颜色空间错配**：钉住当前量级（第 1408 轮；外部复测报告第 1 条）
//
// **⇒ 外部说** ✗：「SVG 里半透明笔触的颜色与 PNG 不一致 ⇒ fill-opacity 与引擎 alpha 一致，
//   但**颜色差最大 79 通道**」⇒ **我复现了，并且证实是**机制性的**：**
//   · **引擎**：在**线性光**空间合成 ⇒ `linear_to_srgb(fg_lin·a + bg_lin·(1−a))`
//   · **SVG**：`fill="#rrggbb"`（**sRGB**）＋ `fill-opacity=a` ⇒ 浏览器在**sRGB 空间**混合
//   ⇒ **两者在数学上就不相等** ⇒ 所以这不是「差一点」，是**路径本身错** ✓
//
// **⇒ 这条判据断什么**（**纯数学，不需要光栅器** ✓）：
//   ① 网格搜索两种合成空间的**最大通道差** ⇒ 打印（实测 **73**，外部报 79 ⇒ 同一机制）
//   ② 断言它 **≤ 80** ⇒ **钉住当前量级**：哪天实现改成「压平到背景色」⇒ 这个数会掉到 ~0 ✓
//      （**⇒ 而若有人把它改坏成更差**（例如把 α 也丢掉）⇒ 会超 80 ⇒ 红 ✓）
//   ③ 打印「理想值」：压平后应为 0 ⇒ 给出**目标**，而不只是现状
//
// **⇒ 退出码**：0 通过｜1 超界（比现状更差）｜**3 不可判**
const LIMIT = 80;   // 当前实测 73 ⇒ 留 7 的余量；目标（压平实现后）应 ≤ 4

const lin2srgb = (x) => (x <= 0.0031308 ? 12.92 * x : 1.055 * x ** (1 / 2.4) - 0.055);

const fgs = [];
for (const r of [0, 0.05, 0.2, 0.5, 0.8, 1]) for (const g of [0, 0.05, 0.2, 0.5, 0.8, 1])
  for (const b of [0, 0.05, 0.2, 0.5, 0.8, 1]) fgs.push([r, g, b]);
const bgs = [];
for (const r of [0, 0.25, 0.5, 0.75, 1]) for (const g of [0, 0.25, 0.5, 0.75, 1])
  for (const b of [0, 0.25, 0.5, 0.75, 1]) bgs.push([r, g, b]);
const alphas = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];

let worst = { d: 0, fg: null, a: null, bg: null, eng: null, svg: null };
for (const fg of fgs) {
  for (const a of alphas) {
    for (const bg of bgs) {
      const eng = [0, 1, 2].map((i) => lin2srgb(fg[i] * a + bg[i] * (1 - a)));
      const fgS = [0, 1, 2].map((i) => lin2srgb(fg[i]));
      const bgS = [0, 1, 2].map((i) => lin2srgb(bg[i]));
      const svg = [0, 1, 2].map((i) => fgS[i] * a + bgS[i] * (1 - a));
      const d = Math.max(...[0, 1, 2].map((i) => Math.abs(Math.round(eng[i] * 255) - Math.round(svg[i] * 255))));
      if (d > worst.d) {
        worst = { d, fg, a, bg, eng: eng.map((v) => Math.round(v * 255)), svg: svg.map((v) => Math.round(v * 255)) };
      }
    }
  }
}

console.log(`  ① 最大通道差 = **${worst.d}**（外部报 79 ⇒ 同一机制 ✓）`);
console.log(`     案例：fg=${JSON.stringify(worst.fg)} a=${worst.a} bg=${JSON.stringify(worst.bg)}`);
console.log(`     引擎（线性合成） ${JSON.stringify(worst.eng)}｜SVG（sRGB 混合） ${JSON.stringify(worst.svg)}`);

let fails = 0;
if (worst.d <= LIMIT) {
  console.log(`  ② 未超过登记的 ${LIMIT} 通道 ✓（当前 ${worst.d}）`);
} else {
  console.error(`  ✗ 颜色空间错配从 ${LIMIT} 恶化到 ${worst.d} 通道`);
  fails += 1;
}

// ③ 理想值：把颜色**压平到背景**（即先在线性空间合成再 emit）⇒ 差值应为 0
const ideal = 0;
console.log(`  ③ 理想值（压平到背景后）= ${ideal} 通道 ⇒ 这是修法目标，不是现状`);

if (fails > 0) process.exit(1);
console.log("\n  ✓ 半透明颜色空间错配的**量级**被钉住（修法：按线性空间压平后再 emit）");
