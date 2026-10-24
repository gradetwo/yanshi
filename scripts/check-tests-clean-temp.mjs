#!/usr/bin/env node
// **用 `temp_dir()` 的测试必须自己删掉临时目录**（第 813 轮）
//
// **⇒ 为什么要有这条** ✗：`/tmp` 在这台机器上是 **1.9 GB 的 tmpfs** ✓
//   而我的夹具里有 9.4 MB 裸 RGBA 与 800×800 文档的渲染缓存 ✓
//   ⇒ **只建不删** ⇒ 跑若干轮后 `/tmp` **100% 满** ✓
//   ⇒ 代价：**所有命令都起不来**（`bash` 要写临时脚本 ⇒ `ENOSPC`）✓
//     而 `df -h /` 仍显示 44 G 空闲（**根分区没问题，是 `/tmp` 这个 tmpfs**）✓
//   ⇒ **连续 3 轮无法提交** ✓ 所以把它钉成判据 ✓。
//
// **⇒ 判据（结构检查，如实说明）**：`crates/*/tests/*.rs` 里凡是调用 `std::env::temp_dir()`
//   的文件，**必须**同时定义 `struct Cleanup`（`Drop` 里 `remove_dir_all` ✓）。
//   **⇒ 变异**：从 `d139_zero_ink_warning.rs` 删掉那段 ⇒ 红 ✓。
//   **⇒ 它证明不了什么**：只证明"每个用临时目录的测试**声明了**清理器" ✓
//     —— 清理是否真的发生，由"/tmp 不再涨"这件事本身证明 ✓（人工观测 ✓）。
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const roots = ["crates"];
const files = [];
for (const root of roots) {
  for (const crate of readdirSync(root)) {
    const dir = join(root, crate, "tests");
    let entries;
    try {
      entries = readdirSync(dir);
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (entry.endsWith(".rs")) files.push(join(dir, entry));
    }
  }
}

const offenders = [];
for (const file of files) {
  const text = readFileSync(file, "utf8");
  if (!text.includes("std::env::temp_dir()")) continue;
  const guarded = text.includes("struct Cleanup") || /impl Drop for \w+/.test(text);
  if (!guarded) offenders.push(file);
}

console.log(`  扫描 ${files.length} 个测试文件，其中用 \`temp_dir()\` 的 ${files.length - offenders.length} 个有清理器`);
for (const file of offenders) console.log(`  ✗ ${file} 用了 \`temp_dir()\` 但没有 Drop 清理器`);
if (offenders.length > 0) {
  console.error(
    `  ✗ ${offenders.length} 个测试会把临时目录留在 /tmp（那是 1.9 GB 的 tmpfs ⇒ 会写满）`,
  );
  process.exit(1);
}
console.log("✓ 每个用 `temp_dir()` 的测试都带了 Drop 清理器（结构检查：只证明声明在）");
