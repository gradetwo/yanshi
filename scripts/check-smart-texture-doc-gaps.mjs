#!/usr/bin/env node
// **`smart_texture` / `direction_from` 的**两个文档缺口**必须写明**（第 1331 轮；外部 `E1317` 换来的）
//
// **⇒ 为什么有这条** ✗：
//   外部（apple2011）第一次用 `smart_texture` 就撞上**空层画不出可见像素**（`E1317`）
//   ⇒ 而当时文档只写「Alpha 永不 1.0」，**没说「空层 ⇒ 画不出来」** ✗
//   ⇒ 他们也发现 `direction_from` 的三个值**都依赖选区**，而文档没写 ⇒ **无选区时失败看不懂** ✗
//
// **⇒ 断什么**（两条，都读 `tools_registry.rs` 的**生产代码**）：
//   ① `smart_texture` 的 summary 必须提到「空（全透明）层」并说明**不会产生可见像素**
//   ② `direction_from` 的说明必须提到**依赖选区**（至少一处，因为有两个工具共用同一句）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync, existsSync } from "node:fs";

const SRC = "crates/yanshi-server/src/tools_registry.rs";
if (!existsSync(SRC)) {
  console.error(`  ⊘ 不可判：找不到 ${SRC}`);
  process.exit(3);
}
const text = readFileSync(SRC, "utf8");
if (text.length < 10000) {
  console.error("  ⊘ 不可判：tools_registry.rs 太短（读取坏了）");
  process.exit(3);
}
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① smart_texture 的 summary：找 `name: "smart_texture"` 到下一个 `summary:` 之间的文本
const idx = text.indexOf('name: "smart_texture"');
if (idx < 0) {
  console.error("  ⊘ 不可判：找不到 smart_texture 的声明");
  process.exit(3);
}
const sumStart = text.indexOf("summary:", idx);
const sumEnd = text.indexOf('",', sumStart);
const summary = text.slice(sumStart, sumEnd);
if (/空（全透明）层/.test(summary) && /不会产生可见像素/.test(summary)) {
  ok("① smart_texture 的 summary 写明了「空层不产生可见像素」");
} else {
  bad("① smart_texture 的 summary 必须写明「空（全透明）层 ⇒ 不会产生可见像素 ⇒ 请先铺底」");
}

// ② direction_from：全文件里必须出现「依赖选区」
const hits = (text.match(/依赖选区/g) ?? []).length;
if (hits >= 1) {
  ok(`② direction_from 的说明提到了「依赖选区」（共 ${hits} 处）`);
} else {
  bad("② direction_from 的说明必须提到「依赖选区（clip_to_selection）⇒ 无选区时失败」");
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（两个文档缺口必须写明）`);
  process.exit(1);
}
console.log("\n  ✓ 两个文档缺口都已写明（空层 / 依赖选区）");
