#!/usr/bin/env node
// **★ 内核产物的**体积上限** ＋ **依赖树里必须含内核本体** ✗ ★**
//   （第 297 轮立「**不得有 `wgpu`**」✓；**第 442 轮按用户裁定改写 ✓**）
//
// **∴ 为什么改写（**用户裁定 ✓）✗ ★**：
//   **∴ 第 297 轮的写法 ✗**：**「**`yanshi-wasm` 的依赖树里**不得**出现 `wgpu`**」 ✓
//     ⇒ **∴ 依据**：**AGENTS.md 第 6 条「**内核体积与确定性**权重更高**」 ✓
//   **∴ 而**第 591 轮**用户裁定**（**`docs/design/gpu-webgpu-discussion.md` §6 ✓）**✗**：
//     ```
//     :68  > 支持 fallback CPU，且参数支持关闭 GPU。做不到一致就牺牲点一致性，
//           ★ wasm 体积也是该牺牲就牺牲。★
//     :80  | 体积 | ★ 接受 `wgpu` 带来的体积增长（**含 wasm 内核**）★
//     ```
//     ＋ **`docs/design/deployment-matrix.md:75`**：**⑤ 内核 WebGPU**（**WebGPU 可用则用；
//       **∴ 体积代价已由用户接受** ✓）** ✓ ★**** ✓✓
//   **★ 所以 ✗ ★**：**字面禁令**与新裁定**冲突** ✓
//     ⇒ **∴ 于是**：**本条**改成**守住**它**真正要守的东西**✗
//       ⇒ **∴ 即**：**「**体积**必须**受控**」** ✓（**∴ 不是**「**不许有 wgpu**」 ✓）★**** ✓✓
//   **★ 两面（**AGENTS.md 第 3 条 ✓）★**：
//     **∴ 收益 ✗**：**内核**可以按裁定**接入 WebGPU**✗（**∴ 离线内核的加速路**打开** ✓）
//     **∴ 代价 ✗**：**体积**会增长**✗（**∴ 已由用户接受 ✓）
//       ⇒ **∴ 而**本条判据**仍挡住**「**失控的增长**」 ✓ ★**** ✓✓
//
// **∴ 判据（两条 ✓）★**：
//   **∴ ①** **内核产物 `crates/yanshi-wasm/pkg/yanshi_wasm_bg.wasm` 的大小 ≤ `LIMIT_BYTES`** ✓
//     （**∴ 缺产物时**明确记为**不可判（退出码 3）**✗ ⇒ **∴ 不许**静默通过** ✓）★**** ✓✓
//   **∴ ②** **`yanshi-wasm` 的依赖树里**必须**含 `yanshi-render`** ✓
//     （**∴ 它**防「**测错了目标**」✗ —— **第 298 轮的教训 ✓）★**** ✓✓
//
// **∴ 变异点 ✗**：**把产物换成一份更大的**（**或**把 `LIMIT_BYTES` 调小**）⇒ **∴ 必红** ✓
//
// 用法：node scripts/tool-wasm-tree-no-wgpu.mjs
import { spawnSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";

// **∴ 体积上限 ✗**：**6 MiB** ✓
//   **∴ 来源 ✗**：**第 442 轮实测**（**当时的产物 ＝ 1,519,384 字节 ＝ 1.45 MiB ✓）
//     ＋ **∴ 给** `wgpu` 接入**留出**余量**✗（**∴ 按裁定**接受增长 ✓）
//       ＋ **∴ 而**上限**挡住**失控** ✓ ★**** ✓✓
const LIMIT_BYTES = 6 * 1024 * 1024;
const WASM_ARTIFACT = "crates/yanshi-wasm/pkg/yanshi_wasm_bg.wasm";

// **★ 先**强制解析**一次 ✗ ★**（第 298 轮 ✓；**∴ 由变异检验逼出来 ✓）：
//   **∴ 症状 ✗**：**只改 `Cargo.toml` ＋ **不**跑解析**✗
//     ⇒ **∴ `cargo tree` **用了**旧的依赖图**✗ ⇒ **∴ 于是**：**变异**没被看见** ✓
//       ⇒ **★ 所以**：**判据会**误绿** ✓ ★**** ✓✓
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
//     ⇒ **∴ 而**原来**用整行去匹配包名**✗ ⇒ **∴ 于是** `includes("yanshi-render")` **是 false** ✓
//       ⇒ **★ 所以**：**第二条断言**（**树里必须有内核本体** ✓）**当场红了** ✓
//     **⇒ ∴ 教训**：**「**必须有**」那条断言**防住了「**测错了目标**」** ✓
const pkgs = [...new Set(
  (r.stdout || "")
    .split("\n")
    .map((l) => l.replace(/\s*\(\*\)\s*$/, "").trim())
    .map((l) => l.split(/\s+/)[0]) // **∴ 包名 ＝ 第一词 ✓**
    .filter((l) => l && l !== "*")
)];
console.log(`  yanshi-wasm 的依赖树：${pkgs.length} 个包`);

let failed = 0;
const check = (ok, name, detail) => {
  if (ok) {
    console.log(`  ✓ ${name}（${detail}）`);
  } else {
    console.error(`  ✗ ${name}（${detail}）`);
    failed += 1;
  }
};

// **∴ 断言 ① ✗**：**内核产物体积 ≤ 上限** ✓
if (!existsSync(WASM_ARTIFACT)) {
  console.error(`  ⊘ 不可判：找不到内核产物 ${WASM_ARTIFACT}`);
  console.error("    ⇒ **∴ 明说**不可判**✗ ⇒ **∴ 不许**静默通过** ✓（**∴ 先**构建 wasm 内核** ✓）");
  process.exit(3);
}
const size = statSync(WASM_ARTIFACT).size;
const mib = (n) => `${(n / 1048576).toFixed(2)} MiB`;
console.log(`  内核产物：${WASM_ARTIFACT} ⇒ ${size} B（${mib(size)}）｜上限 ${mib(LIMIT_BYTES)}`);
check(
  size <= LIMIT_BYTES,
  `内核产物的体积 ≤ 上限（**${mib(LIMIT_BYTES)}**；**∴ 体积代价按第 591 轮裁定接受，但**不许失控** ✗**）`,
  `实测 ${size} B ＝ ${mib(size)}`
);

// **∴ 断言 ② ✗**：**树里必须有内核本体**（**防「**测错了目标**」 ✓）
check(
  pkgs.includes("yanshi-render"),
  "依赖树里必须确实包含 `yanshi-render`（**∴ 否则**本判据**没测到该测的东西** ✓）",
  pkgs.includes("yanshi-render") ? "有" : "无"
);

// **∴ 如实记录 ✗**：**`wgpu` **现在**允许**出现**✗ ⇒ **∴ 若**在 ⇒ **∴ 只**记录 ＋ **报出体积** ✓
const gpuPkgs = pkgs.filter((p) => p === "wgpu" || p.startsWith("wgpu-") || p === "naga");
if (gpuPkgs.length) {
  console.log(`  ℹ️ GPU 家族在树里（**按第 591 轮裁定**允许** ✓）：${gpuPkgs.join("／")}`);
  console.log("     ⇒ **∴ 代价**已写进体积**✗ ⇒ **∴ 上限**仍守住** ✓");
} else {
  console.log("  ℹ️ GPU 家族**不在**树里（**∴ 尚未**接入离线内核** ✓）");
}

console.log("");
if (failed === 0) {
  console.log(`  结论：✓ 内核体积受控（上限 ${mib(LIMIT_BYTES)}）`);
  process.exit(0);
} else {
  console.error(`  结论：✗ 内核体积判据未过（${failed} 条）`);
  process.exit(1);
}
