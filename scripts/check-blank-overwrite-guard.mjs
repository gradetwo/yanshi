#!/usr/bin/env node
// **「全透明补丁不得盖掉已有墨」守卫必须在，且必须在落帧之前**（第 1131 轮；Y-01）
//
// **为什么需要一条静态判据** ✗：这条守卫的**行为**由 `browser-stroke-refresh.mjs` 用连续采样守着 ✓
//   （它会读 `blankBlitsSkipped` ✓），但那条判据属"客户端落笔一族"，**整体红着** ✓
//   ⇒ 守卫哪天被删掉，**红里看不出是它** ✗ ⇒ 本判据只守一件事：
//     **`blitServerBox` 里存在「服务端 0 墨 且 画布有墨 ⇒ 跳过」这一段，并且它在
//     `putImageData` 之前 return** ✓。
//
// **它不证明什么** ✗：不证明像素真的没被盖（那是浏览器判据的事 ✓），
//   只证明**守卫的结构与顺序还在** ✓（删掉 / 挪到落帧之后 ⇒ 红 ✓）。
//
// **变异**（本判据自己的自测会用它们 ✓）：
//   ① 删掉 `if (serverInk === 0)` 整段 ⇒ 红；
//   ② 把守卫挪到 `putImageData` 之后 ⇒ 红（顺序检查）；
//   ③ 去掉 `blankBlitsSkipped` 计数 ⇒ 红（失去可观测信号）。
//
// 用法：node scripts/check-blank-overwrite-guard.mjs [--self-test]
// 退出码：0 = 守卫在且顺序对｜1 = 守卫缺失/顺序错（产品回归）｜3 = 读不到源码（不可判）
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const VIEWER = join(ROOT, "crates/yanshi-http/assets/viewer-app.js");
const PROBE = join(ROOT, "scripts/browser-stroke-refresh.mjs");

/// 判据本身（**可对合成源码跑** ✓ —— `--self-test` 就靠这个 ✓）。
export function judge(source, probeSource) {
  const problems = [];
  const guard = source.indexOf("if (serverInk === 0) {");
  if (guard < 0) {
    problems.push("找不到 `if (serverInk === 0) {` —— 服务端空白时不再做「画布有墨」判断");
  }
  const mainPut = source.indexOf("ctx.putImageData(new ImageData(bytes, w, h)");
  if (mainPut < 0) {
    problems.push("找不到主落帧 `putImageData(new ImageData(bytes, w, h)`");
  }
  if (guard >= 0 && mainPut >= 0 && guard > mainPut) {
    problems.push("守卫在 `putImageData` **之后** ⇒ 先盖后判 ＝ 没守");
  }
  if (guard >= 0) {
    const windowText = source.slice(guard, guard + 1200);
    if (!windowText.includes("canvasInk > 0")) {
      problems.push("守卫里没有 `canvasInk > 0` —— 画布空着也跳过 ⇒ 别处的变更永远贴不上");
    }
    if (!windowText.includes("blankBlitsSkipped")) {
      problems.push("守卫没有记 `blankBlitsSkipped` —— 跳过变成静默");
    }
  }
  if (!probeSource.includes("blankBlitsSkipped")) {
    problems.push("`browser-stroke-refresh.mjs` 不再读 `blankBlitsSkipped` ⇒ 行为侧失去观测点");
  }
  return problems;
}

const selfTest = process.argv.includes("--self-test");
if (selfTest) {
  const probe = "state.blankBlitsSkipped";
  const good = `
    const bytes = new Uint8ClampedArray(4);
    if (serverInk === 0) { const canvasInk = 3; if (canvasInk > 0) { stats.blankBlitsSkipped++; return 0; } }
    ctx.putImageData(new ImageData(bytes, w, h), 0, 0);
  `;
  const noGuard = `
    const bytes = new Uint8ClampedArray(4);
    ctx.putImageData(new ImageData(bytes, w, h), 0, 0);
  `;
  const afterPut = `
    const bytes = new Uint8ClampedArray(4);
    ctx.putImageData(new ImageData(bytes, w, h), 0, 0);
    if (serverInk === 0) { const canvasInk = 3; if (canvasInk > 0) { stats.blankBlitsSkipped++; return 0; } }
  `;
  const noCounter = `
    const bytes = new Uint8ClampedArray(4);
    if (serverInk === 0) { const canvasInk = 3; if (canvasInk > 0) { return 0; } }
    ctx.putImageData(new ImageData(bytes, w, h), 0, 0);
  `;
  const cases = [
    ["正例（守卫在落帧前）", good, probe, []],
    ["变异①删守卫", noGuard, probe, ["找不到"]],
    ["变异②守卫挪到落帧后", afterPut, probe, ["之后"]],
    ["变异③去掉计数", noCounter, probe, ["blankBlitsSkipped"]],
    ["变异④判据不读计数", good, "state = {}", ["不再读"]],
  ];
  let bad = 0;
  for (const [name, src, prb, want] of cases) {
    const got = judge(src, prb);
    const ok = want.length === 0 ? got.length === 0 : want.every((w) => got.some((g) => g.includes(w)));
    if (!ok) {
      bad += 1;
      console.log(`  ✗ 自测：${name}｜期望 ${JSON.stringify(want)}｜实测 ${JSON.stringify(got)}`);
    } else {
      console.log(`  ✓ 自测：${name}`);
    }
  }
  process.exit(bad === 0 ? 0 : 1);
}

let source;
let probeSource;
try {
  source = readFileSync(VIEWER, "utf8");
  probeSource = readFileSync(PROBE, "utf8");
} catch (error) {
  console.error(`  ⊘ 不可判：读不到源码（${error.message}）（不是产品问题）`);
  process.exit(3);
}
const problems = judge(source, probeSource);
if (problems.length > 0) {
  console.error(`✗ 「全透明补丁不得盖掉已有墨」的守卫不成立（${problems.length} 条）：`);
  for (const problem of problems) console.error(`   - ${problem}`);
  process.exit(1);
}
console.log("✓ 全透明补丁的守卫在，且先判后落帧；跳过有 `blankBlitsSkipped` 计数，行为侧判据也在读");
