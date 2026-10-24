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
  // **⇒ 只声明不够，还要**真的绑上** ✅（第 1003 轮：原版只证明"声明在" ✗）
  //   **⇒ 为什么** ✗：声明一个 `Drop` 类型却**从不实例化** ⇒ 目录照样不删 ✓
  //     （第 939 轮就是这么干的：先只加声明，后来才在 26 处调用点绑定 ✓）
  //   **⇒ 判法**：文件里必须出现**守卫绑定**（`= Cleanup(` 或 `= TempCleanup(` ✓）
  //   **⇒ 变异**：把某文件的绑定删掉（只留声明）⇒ 该文件红 ✓
  // **⇒ 两种合法形态** ✅（第 1003 轮实测发现第二种）：
  //   ① 调用点绑定：`let _g = Cleanup(dir.clone());`
  //   ② **助手直接返回守卫**：`fn temp_dir(..) -> TempRoot { … TempRoot(path) }` ✓
  //      ⇒ 这时**绑定在调用方**（`let root = temp_dir(..)` ✓）⇒ 同样会 Drop ✓
  //   **⇒ 只看 ① 会误报** ✗（第 1003 轮实测误报 2 个 ✓）⇒ 所以两种都认 ✓
  const bound = /[\w_]+\s*=\s*(?:Temp)?Cleanup\s*\(/.test(text)
    || /fn\s+\w*temp_dir\w*\s*\([^)]*\)\s*->\s*(?:Temp)\w+/.test(text);
  if (!guarded) { offenders.push(`${file}（没有 Drop 清理器）`); continue; }
  if (!bound) offenders.push(`${file}（有 Drop 类型但**从未绑定**）`);
}

console.log(`  扫描 ${files.length} 个测试文件，其中用 \`temp_dir()\` 的 ${files.length - offenders.length} 个有清理器`);
for (const file of offenders) console.log(`  ✗ ${file} 用了 \`temp_dir()\` 但没有 Drop 清理器`);
if (offenders.length > 0) {
  console.error(
    `  ✗ ${offenders.length} 个测试会把临时目录留在 /tmp（那是 1.9 GB 的 tmpfs ⇒ 会写满）`,
  );
  process.exit(1);
}
console.log("✓ 每个用 `temp_dir()` 的测试都带了 Drop 清理器，**且守卫真的被绑定**（两种形态：调用点绑定／助手返回守卫）");
