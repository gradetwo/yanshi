#!/usr/bin/env node
// **掩膜遍与主遍必须共用同一套时间基**（第 1417 轮；外部 D-10）
//
// **⇒ 原缺陷** ✗：`brush_deposit_mask`（只取"哪些像素落过墨"的第二遍）原来调
//   **不带 `dtimes` 的 `stamp_stroke`** 包装 ⇒ 那个包装固定传 `&[]` ⇒ 每步按 `STEP_SECONDS` 走
//   ⇒ 而引擎每步的 dab 数含 `dt × dabs_per_second` ⇒ **有真实时间戳时两遍的 dab 不是同一批** ✗
//   ⇒ **而 `stamp_stroke` 的注释自己写着**「两遍的 dab 位置/半径/压力**必须完全一致**」
//   ⇒ **注释对、代码错**（与 `fill_region.opacity` 同一类）✗
//
// **⇒ 这条判据是**静态**的，但它**是可靠的**（与「参数是否被读」那类不同 ✗）：
//   因为它检查的是一个**具体的调用关系**：`brush_deposit_mask` 内部**不许**再出现
//   `stamp_stroke(`（无 `dtimes` 的包装）⇒ 只能出现 `stamp_stroke_from(` ＋ `dtimes` ✓
//
// **⇒ 断什么**：
//   ① `brush_deposit_mask` 的函数体里**不许**出现 `stamp_stroke(`（要出现 `stamp_stroke_from(`）
//   ② 函数体里**必须**引用 `dtimes`
//   ③ 打印两处调用点（`tools_brush.rs` 传实际 dtimes、`tools_mask.rs` 传 `&[]` 因为那条路没有时间戳）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync } from "node:fs";

const FILES = ["crates/yanshi-server/src/tools.rs", "crates/yanshi-server/src/tools_brush.rs",
  "crates/yanshi-server/src/tools_mask.rs"];
let texts;
try {
  texts = FILES.map((f) => readFileSync(f, "utf8"));
} catch {
  console.error("  ⊘ 不可判：读不到源文件");
  process.exit(3);
}
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// 取 brush_deposit_mask 的函数体（按大括号配平）
const src = texts[0];
const at = src.indexOf("fn brush_deposit_mask(");
if (at < 0) {
  bad("① 找不到 brush_deposit_mask（函数被改名了？这条判据需要跟着改）");
} else {
  let depth = 0; let i = src.indexOf("{", at); let j = i;
  while (j < src.length) {
    if (src[j] === "{") depth += 1;
    else if (src[j] === "}") { depth -= 1; if (depth === 0) break; }
    j += 1;
  }
  const body = src.slice(i, j + 1);
  if (/\bstamp_stroke\s*\(/.test(body)) {
    bad("① 掩膜遍里出现了不带 dtimes 的 stamp_stroke ⇒ 两遍时间基不同（这正是 D-10）");
  } else if (/\bstamp_stroke_from\s*\(/.test(body)) {
    ok("① 掩膜遍走 stamp_stroke_from（没有无 dtimes 的包装）");
  } else {
    bad("① 掩膜遍里既没有 stamp_stroke 也没有 stamp_stroke_from ⇒ 结构变了，判据要跟着改");
  }
  if (/\bdtimes\b/.test(body)) ok("② 掩膜遍引用了 dtimes");
  else bad("② 掩膜遍必须引用 dtimes（否则又回到固定 STEP_SECONDS）");
}

// ③ 打印两处调用点
const brush = texts[1];
const mask = texts[2];
const bCall = /brush_deposit_mask\([^)]*\)/.exec(brush);
const mCall = /brush_deposit_mask\([^)]*\)/.exec(mask);
console.log(`  ③ 调用点：tools_brush.rs ⇒ ${bCall ? bCall[0].replace(/\s+/g, " ") : "未找到"}`);
console.log(`  ③ 调用点：tools_mask.rs  ⇒ ${mCall ? mCall[0].replace(/\s+/g, " ") : "未找到"}`);
if (bCall && !/&dtimes|dtimes/.test(bCall[0])) bad("③ tools_brush.rs 那处必须把实际 dtimes 传进去");
else if (bCall) ok("③ tools_brush.rs 传的是实际 dtimes");

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（掩膜遍必须与主遍共用时间基）`);
  process.exit(1);
}
console.log("\n  ✓ 掩膜遍与主遍共用同一套时间基（D-10 不会复发）");
