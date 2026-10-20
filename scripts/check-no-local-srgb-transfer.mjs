#!/usr/bin/env node
// **服务端不许再定义自己的 sRGB 传递函数** ✓
// （色彩体系规划 Phase 1「工具层颜色参数统一走公共转换入口（收敛散落逻辑）」）
//
// **∴ 问题**：`crates/yanshi-server/src/tools.rs` 曾**本地定义** `srgb_to_linear` 与
//   `linear_to_srgb`，而 `crates/yanshi-render/src/color.rs` 有**同名同义**的公共版本。
//   ⇒ 两份实现**已经不同**：公共版 `srgb_to_linear` 有 `clamp(0,1)`，本地版没有
//   ⇒ 于是「越界输入」在两个调用点给出**不同**结果 ⇒ 这就是「散落逻辑」的具体形态 ✗
//
// **∴ 这条判据守什么**（2 条断言）：
//   ① `tools.rs` 里**不许有** `fn srgb_to_linear` / `fn linear_to_srgb` 的定义
//   ② 那些调用必须**来自** `yanshi_render::color`（不许再出现无来源的本地实现）
//
// **∴ 变异**：把本地函数加回去 ⇒ ① 必红 ✓
import { readFileSync } from "node:fs";

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

const FILES = [
  "crates/yanshi-server/src/tools.rs",
];
// 去掉注释（我这两轮已经两次把注释当代码，这里一开始就剥离）
const strip = (text) =>
  text
    .split("\n")
    .map((line) => {
      const i = line.indexOf("//");
      return i >= 0 ? line.slice(0, i) : line;
    })
    .join("\n");

for (const file of FILES) {
  let src = "";
  try {
    src = strip(readFileSync(file, "utf8"));
  } catch {
    console.error(`  ⊘ 不可判：读不到 ${file}`);
    process.exit(3);
  }
  const defs = [...src.matchAll(/fn\s+(srgb_to_linear|linear_to_srgb)\s*\(/g)].map((m) => m[1]);
  check(
    defs.length === 0,
    `① ${file} 里不许本地定义 sRGB 传递函数（实测 ${defs.length} 处${defs.length ? `：${defs.join(", ")}` : ""}）`,
  );
}

// ② 调用必须来自公共 crate（`use yanshi_render::color::…` 或带前缀调用）
const tools = strip(readFileSync("crates/yanshi-server/src/tools.rs", "utf8"));
const hasUse = /use\s+yanshi_render::color::\{[^}]*\b(srgb_to_linear|linear_to_srgb)\b[^}]*\}/.test(tools);
const hasPrefixed = /yanshi_render::color::(srgb_to_linear|linear_to_srgb)\s*\(/.test(tools);
check(
  hasUse || hasPrefixed,
  `② 服务端必须通过 yanshi_render::color 取这两个函数（use 或带前缀调用）`,
);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（sRGB 传递函数必须只有一份）`);
  process.exit(1);
}
console.log("\n  ✓ 服务端不再自带 sRGB 传递函数，统一走 yanshi_render::color");
