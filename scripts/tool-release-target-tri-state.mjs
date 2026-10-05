#!/usr/bin/env node
// 判据：`target_installed()` 的**三态**必须分开（0=已安装，1=明确没装，2=无法判断）。
//
// 为什么需要它（有历史 bug 支撑）：老版本在没有 rustup 时回 0（当成已装）⇒
// 调用方放心去构建 ⇒ 后面才炸，而且报的是**误导性错误**（看着像工具链坏，其实根本没装）。
// 当时的静态判据全绿，只有发布包才暴露 —— 所以这条必须能红。
//
// 判据（都能红）：用**假的 HOME** 造三种文件系统状态，断言 return code 分别是 0 / 1 / 2。
// 只提取该函数本身（sed 取 `target_installed()` 到第一个 `}`），不跑脚本主体。
// 变异：把函数里的 `return 2` 改成 `return 0` ⇒ 第三种情况断言失败 ⇒ 判据红。

import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// **用判据自身的位置解析仓库根** ✗（第 1128 轮）：原来用相对路径 `scripts/package-release.sh`，
// 于是 cwd 不同（runner 可能在别处跑，或把脚本复制走）就会找不到文件；
// 这样"变异后仍绿"就可能是**没读到我变异的那个文件**，而不是判据真的能红。
// 本文件位于 <repo>/scripts/，所以仓库根是它的上一级 ✓。
const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SCRIPT = join(REPO_ROOT, "scripts/package-release.sh");
const failures = [];
const TARGET = "x86_64-unknown-linux-musl";

// 1) 只取函数体，避免跑脚本主体。
let fn = "";
try {
  fn = execFileSync("sed", ["-n", "/^target_installed()/,/^}/p", SCRIPT], { encoding: "utf8" });
} catch (error) {
  failures.push(`从 ${SCRIPT} 提取 target_installed() 失败：${error}`);
}
if (!fn.includes("target_installed()")) failures.push("提取到的片段里没有 target_installed()");

const callWith = (home) => {
  const probe = join(home, "probe.sh");
  writeFileSync(probe, `${fn}\ntarget_installed "${TARGET}"\necho "RC=$?"\n`);
  const out = execFileSync("bash", [probe], { encoding: "utf8", env: { ...process.env, HOME: home } });
  const m = /RC=(\d+)/.exec(out);
  return m ? Number(m[1]) : null;
};

const root = mkdtempSync(join(tmpdir(), "yanshi_ti_"));
try {
  // 情况 A：目标目录存在 ⇒ 已安装 ⇒ 0
  const a = join(root, "a");
  mkdirSync(join(a, ".rustup", "toolchains", "stable-x86_64-unknown-linux-gnu", "lib", "rustlib", TARGET), { recursive: true });
  const rcA = callWith(a);

  // 情况 B：有 rustup 目录但没有该目标 ⇒ 明确没装 ⇒ 1
  const b = join(root, "b");
  mkdirSync(join(b, ".rustup", "toolchains"), { recursive: true });
  const rcB = callWith(b);

  // 情况 C：没有 rustup（系统 cargo / Homebrew rust）⇒ 无法判断 ⇒ 2
  const c = join(root, "c");
  mkdirSync(c, { recursive: true });
  const rcC = callWith(c);

  console.log(`  · 三态读数：已安装=${rcA} 明确没装=${rcB} 无法判断=${rcC}`);
  if (rcA !== 0) failures.push(`目标目录存在时应当回 0（已安装），实得 ${rcA}`);
  if (rcB !== 1) failures.push(`有 rustup 但缺目标时应当回 1（明确没装），实得 ${rcB}`);
  if (rcC !== 2) failures.push(`没有 rustup 时应当回 2（**无法判断**，不能当成已安装），实得 ${rcC}`);
} finally {
  rmSync(root, { recursive: true, force: true });
}

if (failures.length) {
  console.log("  ✗ 三态判据未通过：");
  for (const f of failures) console.log(`    - ${f}`);
  process.exit(1);
}
console.log("  ✓ target_installed 三态分开（0/1/2）");
