#!/usr/bin/env node
// **`set_brush_dynamics` 的映射覆盖** ✓（第 1000+ 轮 ✓；Muse 的 P1-4 ✓）
//
// **∴ 事实（扫全部 `.myb`）✓**：真实存在的 `(setting, input)` 组合有 **187** 个 ✓，
//   而 `tools.rs` 的 `MAPPING` 覆盖 **7** 个 ✓ ⇒ **∴ 覆盖率约 3.7% ✗**
//   ⇒ **∴ 所以 Muse 的「60 项 inputs 曲线不可写」**方向是对的**✓**（**∴ 具体数字不同 ✓，量级相近 ✓**）。
//   **∴ 我自己一度写"只有 5 个组合"✗ —— 那是**只看了 `oil-06-paint` 一个文件**✗ ⇒ **∴ 已纠正 ✓**。
//
// **∴ 这条判据守什么 ✓**：**覆盖不可能是 100%** ✗ ⇒ **∴ 守"不许变少"✗**：
//   ① `MAPPING` 条数 ≥ 下限 ✓；② 打印真实覆盖率 ✓；③ 列出缺口（**∴ 已知缺口 ✗，不是 bug ✓**）。
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

const MIN_MAPPED = 7;
const root = new URL("../", import.meta.url);
const src = readFileSync(fileURLToPath(new URL("crates/yanshi-server/src/tools.rs", root)), "utf8");
const block = src.match(/const MAPPING: \[\(&str, &str, &str\); \d+\] = \[([\s\S]*?)\n    \];/);
if (!block) { console.log("  ✗ 在 tools.rs 里找不到 MAPPING 表"); process.exit(1); }
const mapped = [...block[1].matchAll(/\("([^"]+)",\s*"([^"]+)",\s*"([^"]+)"\)/g)]
  .map((m) => `${m[2]}/${m[3]}`);
const have = new Set(mapped);

const wanted = new Set();
const dir = fileURLToPath(new URL("assets/brushes/", root));
for (const name of readdirSync(dir).filter((f) => f.endsWith(".myb"))) {
  let doc;
  try { doc = JSON.parse(readFileSync(`${dir}${name}`, "utf8")); } catch { continue; }
  for (const [setting, value] of Object.entries(doc.settings ?? {})) {
    for (const input of Object.keys(value?.inputs ?? {})) wanted.add(`${setting}/${input}`);
  }
}
if (wanted.size === 0) { console.log("  ✗ 没从 .myb 里读到任何 inputs ⇒ 判据跑不了"); process.exit(3); }

const missing = [...wanted].filter((w) => !have.has(w));
console.log(`  .myb 里真实存在的 (setting,input) 组合：**${wanted.size}** 个 ✓`);
console.log(`  MAPPING：**${mapped.length}** 条 ✓`);
if (mapped.length < MIN_MAPPED) {
  console.log(`  ✗ MAPPING 只有 ${mapped.length} 条，少于下限 ${MIN_MAPPED}`);
  process.exit(1);
}
console.log(`  ✓ MAPPING 有 ${mapped.length} 条 ≥ 下限 ${MIN_MAPPED}`);
console.log(`  **覆盖率 = ${have.size} / ${wanted.size} = ${(have.size / wanted.size * 100).toFixed(1)}%**`);
if (missing.length) {
  console.log(`  还有 ${missing.length} 个真实组合没映射（**已知缺口 ✓，不是 bug ✗**）⇒ 前 6 个：`);
  for (const m of missing.slice(0, 6)) console.log(`     - ${m}`);
}
console.log("  ✓ set_brush_dynamics 的映射没有变少（覆盖率的缺口已列在上面）");
