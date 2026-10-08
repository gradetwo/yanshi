#!/usr/bin/env node
// **★ 每个 `scripts/*.mjs` 都必须能被 `node --check` 解析 ✓ ★**
//
// **为什么要单独一条判据** ✗（第 438 轮实测 ✓）：
// 我把 `scripts/browser-ui-check.mjs` 用正则清理探针时**写坏了它的语法** ✗，
// **而 `cargo fmt` ＋ `cargo clippy` ＋ `cargo test` **都不检查 JS** ✗** ⇒
// **∴ 那个坏文件**静默进了 main ✗** ⇒ **∴ 直到下一次真正跑它才会暴露 ✓**。
//
// **判据（可红 ✓）**：**对每个 `scripts/*.mjs` 跑 `node --check`** ⇒ **∴ 有任何一个解析失败 ⇒ **非零退出 ✓**。
// **变异** ✗：**故意在任一脚本里制造语法错 ⇒ **必红 ✓**（**已实测：它抓住了我自己的那次错误 ✓**）。
import { readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";

const dir = "scripts";
const files = readdirSync(dir).filter((f) => f.endsWith(".mjs")).sort();
const bad = [];
for (const f of files) {
  try {
    execFileSync(process.execPath, ["--check", `${dir}/${f}`], { stdio: "pipe" });
  } catch (error) {
    const out = String((error && error.stderr) || (error && error.message) || error);
    bad.push({ file: f, first: out.split("\n").slice(0, 3).join(" | ").slice(0, 200) });
  }
}
if (bad.length) {
  console.error("❌ 有 " + bad.length + " 个 scripts/*.mjs 语法不过（cargo 的三种门禁都不查 JS ✗）：");
  for (const b of bad) console.error("   - " + b.file + " ⇒ " + b.first);
  process.exit(1);
}
console.log("✓ " + files.length + " 个 scripts/*.mjs 全部通过 node --check");
