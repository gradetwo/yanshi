#!/usr/bin/env node
// **★ `yanshi-wasm` 的依赖树里**不得**出现 `wgpu` ✗ ★**（第 297 轮 ✓；**目标第 6／7 条 ✓）。
//
// **∴ 为什么必须有这条 ✗**：**第 295／296 轮**的架构决定**（**如实 ✓）**✗**：
//   **∴ 我**权衡过「**把 `wgpu` 放进 `yanshi-render`**」**✗
//     ⇒ **∴ 发现**：**`yanshi-render` **也被 `yanshi-wasm` 依赖**** ✓
//       ⇒ **★ 所以**：**那**会让 `wgpu` **进入 wasm 的依赖图** ✓ ★**** ✓✓
//   **∴ 而**AGENTS.md 第 6 条 ＋ 目标第 6 条都说：
//     **"**`yanshi-wasm`（**浏览器内核 ✓）的体积与确定性**权重更高** ✓
//     ⇒ **∴ 所以**：**那条路**代价更大** ✓**** ✓✓
//   **⇒ ★ 因此**：**wasm 的依赖树**必须**干净** ✗
//     ⇒ **∴ 本判据**守住它** ✓ ★**** ✓✓
//
// **∴ 变异点 ✗**：**把 `wgpu` 加成 `yanshi-render`（**或 `yanshi-wasm` ✓）的**非可选依赖**✗
//   ⇒ **∴ 树里**立刻出现 `wgpu`**✗ ⇒ **∴ 必红** ✓**** ✓✓
//
// 用法：node scripts/tool-wasm-tree-no-wgpu.mjs

import { spawnSync } from "node:child_process";

// **∴ 禁词 ✗**：**`wgpu` 家族 ＋ 它带进来的图形栈** ✓
const BANNED = ["wgpu", "wgpu-core", "wgpu-hal", "wgpu-types", "naga", "ash", "d3d12", "metal"];

// **★ 先**强制解析**一次 ✗ ★**（第 298 轮 ✓；**∴ 由变异检验逼出来 ✓）：
//   **∴ 症状 ✗**：**只改 `Cargo.toml` ＋ **不**跑解析**✗
//     ⇒ **∴ `cargo tree` **用了**旧的依赖图**✗ ⇒ **∴ 于是**：**变异（**加 `wgpu` ✓）**没被看见** ✓
//       ⇒ **★ 所以**：**判据会**误绿** ✓ ★**** ✓✓
//   **∴ 修法 ✗**：**先跑 `cargo metadata`**✗（**∴ 它**会**按当前 `Cargo.toml` 解析 ✓）
//     ⇒ **∴ 然后** `cargo tree` **才反映真实依赖** ✓**** ✓✓
spawnSync("cargo", ["metadata", "--format-version", "1"], {
  encoding: "utf8",
  maxBuffer: 1 << 28,
  env: { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR || "target" },
});

const r = spawnSync("cargo", ["tree", "-p", "yanshi-wasm", "-e", "normal", "--prefix", "none"], {
  encoding: "utf8",
  maxBuffer: 1 << 28,
  env: { ...process.env, CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR || "target" },
});
if (r.status !== 0) {
  console.error(`  ✗ \`cargo tree\` 失败（退出码 ${r.status}）⇒ **∴ 本跑没有结论**`);
  console.error((r.stderr || "").split("\n").slice(0, 4).join("\n"));
  process.exit(2);
}
// **★ 解析要**取包名本身** ✗ ★**（第 298 轮 ✓；**∴ 由第二条断言逼出来 ✓）：
//   **∴ 实测的输出形 ✗**：**`yanshi-render v0.1.0 (/home/crow/yanshi/crates/yanshi-render)`** ✓
//     ⇒ **∴ 而**我原来**用整行去匹配包名**✗ ⇒ **∴ 于是** `includes("yanshi-render")` **是 false** ✓
//       ⇒ **★ 所以**：**第二条断言**（**树里必须有 `yanshi-render`** ✓）**当场红了** ✓ ★**** ✓✓
//     **⇒ ∴ 教训**：**「**必须有**」那条断言**防住了「**测错了目标**」** ✓**** ✓✓
//   **∴ 现在**：**每行取**第一个词**✗ ⇒ **∴ 并**去掉重行标记 `(*)`** ✓**** ✓✓
const pkgs = [...new Set(
  (r.stdout || "")
    .split("\n")
    .map((l) => l.replace(/\s*\(\*\)\s*$/, "").trim())
    .map((l) => l.split(/\s+/)[0]) // **∴ 包名 ＝ 第一词 ✓**
    .filter((l) => l && l !== "*")
)];
console.log(`  yanshi-wasm 的依赖树：${pkgs.length} 个包`);

const hits = pkgs.filter((p) => BANNED.some((b) => p === b || p.startsWith(b + "-")));
for (const h of hits) {
  console.log(`    ✗ ${h}`);
}

let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
check(hits.length === 0,
  "`yanshi-wasm` 的依赖树里不得出现 `wgpu` 家族（**内核体积权重更高**）",
  `实测 ${hits.length} 个：${hits.slice(0, 6).join(", ") || "无"}`);

// **∴ 顺带：树里必须有 `yanshi-render`** ✓（**∴ 否则**这个判据**测错了目标** ✓）
check(pkgs.includes("yanshi-render"),
  "树里必须**确实**包含 `yanshi-render`（**∴ 否则**本判据**没测到该测的东西** ✓）",
  `实测 ${pkgs.includes("yanshi-render") ? "有" : "无"}`);

console.log("");
if (failed === 0) { console.log("  结论：✓ wasm 的依赖树与 GPU 无关"); process.exit(0); }
console.error(`  结论：✗ ${failed} 条 ⇒ **∴ GPU 泄漏进内核依赖图** ✗ ⇒ **∴ 内核体积会涨** ✓`);
process.exit(1);
