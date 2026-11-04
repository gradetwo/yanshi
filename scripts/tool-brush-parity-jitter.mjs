#!/usr/bin/env node
// **带随机的笔刷必须逐字节相同** 判据（第 280 轮 ✓）—— 给"`dx`/`dy` 对齐"那条修复配一条**能红**的判据 ✓。
//
// **为什么需要它** ✗：
//   第 278 轮查明：**带非零 `dx`/`dy` 的 `stroke_to` 在这个 hokusai 版本下一枚印章都不落** ✓，
//   而**服务端一直把 `dx`/`dy` 乘 0** ✓、**内核没乘** ✗ ⇒ 于是"同一条笔触在两种渲染模式下"：
//     * 门面**整块全零** ✗（`有墨首像素 #-1`），每色差异 **5757~11515 字节** ✗；
//     * 而**服务端**有墨 ✓。
//   第 279 轮把内核也对齐 ✓ ⇒ 实测：
//     * `8B_Pencil#1` 五色**全部逐字节相同** ✓（首个有墨像素与服务端**完全相同** ✓）；
//     * 全量 199 支：相同 **510 → 535**、差异 **41 → 16** ✓
//       —— 而 **+25 正好是 5 支 × 5 色** ✓。
//
// **为什么不能只用 `kernel-brush-parity.mjs`** ✗：它**改前红（41）／改后仍红（16）** ✓
//   ⇒ **∴ 它无法用"转绿"证明这次修复** ✓ ⇒ 需要一条**聚焦**断言 ✓。
//
// **判据（一条，能红 ✓）**：对**这 5 支**笔刷跑**同一条**判据（`kernel-brush-parity.mjs` ✓
//   —— **调用它**，不重写它的比对逻辑 ✓），要求它**退出码 0** ✓
//   ⇒ 即"这 5 支 × 4 色**全部逐字节相同**" ✓。
//
// **为什么是这 5 支** ✓：它们是**带随机**的笔刷里、**被 `dx`/`dy` 分歧整幅毁掉**、**现已全部相同**的那些 ✓
//   （`8B_Pencil#1` / `Fountain_SF#1` / `Fount-offset#1` / `marker_fat` / `marker_small` ✓）
//   —— 它们正是"同一条笔触在两种渲染模式下逐字节相同"这条性质的**直接体现** ✓。
//   **不写死计数** ✗（"535"／"16" 是某次观测 ✓）⇒ 断言只落在**相等性**上 ✓。
//
// **变异检验** ✓：把内核的 `dx`/`dy` 改回**真实增量**（即撤销第 279 轮的修复 ✓）
//   ⇒ 这 5 支会重新整幅不同 ⇒ 子判据退出 1 ⇒ **本判据变红** ✓✓。
//
// 用法：node scripts/tool-brush-parity-jitter.mjs <base-url> <doc> <token>
//   （与其它 `tool-*` 判据同形 ✓，`run-criteria.sh:192` 就是这么调的 ✓。）

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";

const base = process.argv[2];
const doc = process.argv[3];
const token = process.argv[4];
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-parity-jitter.mjs <base-url> <doc> <token>");
  process.exit(2);
}

// 与 `run-criteria.sh` 同一个包路径 ✓（相对仓库根 ✓）。
// **缺产物 ⇒ 前提不成立 ⇒ `exit(3)`** ✓（原 `exit(2)` ✗：2 是「用法错」的码 ⇒ 会被读成
// 「我调错了参数」✗ 而不是「没构建 wasm」✗ ⇒ 与产品红同样分不开 ✓）。
const pkg = "crates/yanshi-wasm/pkg/yanshi_wasm.js";
if (!existsSync(pkg)) {
  console.error(
    `  ⊘ 不可判：找不到 ${pkg}（先 \`cargo build -p yanshi-wasm --target wasm32-unknown-unknown --release\` ＋ \`wasm-bindgen\`，或 \`make wasm\`）`,
  );
  process.exit(3);
}

/// 这 5 支：带随机、曾被 `dx`/`dy` 分歧毁掉、现已全部逐字节相同 ✓（见文件头 ✓）。
const BRUSHES = [
  "8B_Pencil#1",
  "Fountain_SF#1",
  "Fount-offset#1",
  "marker_fat",
  "marker_small",
];

// **调用同一条判据** ✓（不复制它的比对逻辑 ✓ ⇒ 不留第二份实现 ✗）。
const child = spawnSync(
  process.execPath,
  ["scripts/kernel-brush-parity.mjs", base, doc, token, pkg, ...BRUSHES],
  { encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 90_000 },   // ★ 加超时（10 分钟 ✓）：spawnSync 是**同步**的 ⇒ 子进程挂住会**占住事件循环** ⇒ 上面的 setTimeout 守卫**烧不到** ✗ ⇒ 分片会永久挂住 ✓
);

const out = (child.stdout || "") + (child.stderr || "");
if (child.error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 子判据起不来或超时 ⇒ " + child.error.message + " ⇒ 退出码 3");
  process.exit(3);   // ★ 判据跑不了 ⇒ 退 3（原退 2 ✗：那是"用法错"码 ✓）
}
// 把每支的判定行照印 ✓（读的人一眼看到"相同/有差异" ✓）。
for (const line of out.split("\n")) {
  if (/^\s{2}\S+\/(red|grey|blue|white|black)\b/.test(line)) {
    console.log("  " + line.trim());
  }
}
// **子判据说「前提不成立」⇒ 本判据也**不可判** ✓（不能把 3 翻译成 1 ✗）：**
//   子判据的 exit(3) 有明确语义（0 绿｜1 产品红｜3 不可判｜124 超时 ✓）⇒ 透传 ✓。
//   ⚠️ **必须放在「有没有结论行」之前** ✓：子判据**先**判前提时**不会**打结论行 ✗
//   ⇒ 若先查结论，这条就永远走不到（死代码 ✗），而 3 会被下面翻成 1 ✗。
if (child.status === 3) {
  console.error("  ⊘ 不可判：子判据报了「前提不成立」（退出码 3）⇒ 不是笔刷差异，是前提没成立");
  process.exit(3);
}
const conclusion = out.split("\n").filter((line) => line.includes("结论")).pop();
if (!conclusion) {
  // **子判据没打结论 ⇒ 它自己不可判** ⇒ 3 ✓（不是产品红 ✗：`exit(2)` 会被读成「我调错了参数」✗）。
  console.error(
    "  ⊘ 不可判：子判据没打结论行（退出码 " + child.status + "）⇒ 判据无法运行，不是笔刷差异",
  );
  console.error(out.split("\n").slice(-8).join("\n"));
  process.exit(3);
}
console.log("  子判据结论：" + conclusion.trim());

if (child.status !== 0) {
  console.error(
    `结论：带随机的笔刷没有逐字节相同 ✗（子判据退出码 ${child.status}）` +
    "⇒ 两端在落笔链上又不一致了（`dx`/`dy` 对齐被撤销？）",
  );
  process.exit(1);
}
console.log(`结论：✓ 这 ${BRUSHES.length} 支带随机的笔刷在两种渲染模式下逐字节相同`);
process.exit(0);
