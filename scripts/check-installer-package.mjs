#!/usr/bin/env node
// **判据：三平台安装包的接线不能漂移**（静态检查，只读文件 + 跑 --print-name）。
//
// **为什么需要它**：安装包（.msi/.dmg/.deb）由 `scripts/package-installer.sh` 构建，
// release.yml 与 nightly.yml 两条线都调它。若有人改了一条线的 YAML（删了构建步、
// 计数断言没跟上、产物没进上传集合），另一条线**照样绿** ⇒ 用户在某条线上拿不到
// 安装包，而没有任何信号。本判据把「两条线都接上、且计数口径一致」钉住。
//
// **判据（都能红）**：
//   ① `scripts/package-installer.sh` 存在；
//   ② 五个发布目标的 `--print-name` 输出与稳定模板逐字一致
//      （`yanshi-<版本>-<target>.msi|.dmg|.deb`，扩展名按平台族对：
//      windows→msi、apple-darwin→dmg、linux→deb）——这是后续 Homebrew cask /
//      AUR PKGBUILD 拼下载 URL 的契约，改名即红；
//   ③ release.yml 与 nightly.yml 都有**带参数的实际调用行**
//      （`package-installer.sh --archive …`；只在注释里提名字不算——第一版判据
//      就栽在这里：变异删掉调用行后，头部注释里的脚本名让它照样绿），且都把
//      .msi/.dmg/.deb 写进了产物上传集合；
//   ④ 两条线的校验和计数断言都把安装包算进去（文本里有 `n_installers`）——
//      断言若只数 tar.gz，加了安装包的那天起发布就会被自己的断言拦死。
//
// **变异**：把 nightly.yml 里 `package-installer.sh` 的调用删掉 ⇒ ③ 红；
//           把脚本里 linux 的扩展名改成 zip ⇒ ② 红。
// **它证明什么（如实）**：只证明接线与命名契约在，不证明安装包真能装——
// 那由两条发布线各自的校验步实证（msiexec /a、hdiutil 挂载、dpkg -i 后打 /health）。

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SCRIPT = join(REPO_ROOT, "scripts/package-installer.sh");
const failures = [];

if (!existsSync(SCRIPT)) {
  failures.push("scripts/package-installer.sh 不存在 ⇒ 安装包没有构建入口");
} else {
  // ② 命名契约：版本用一个假值 9.9.9（与仓库当前版本无关 ⇒ 模板变了必然红）。
  const CASES = [
    ["x86_64-unknown-linux-gnu", "yanshi-9.9.9-x86_64-unknown-linux-gnu.deb"],
    ["aarch64-unknown-linux-gnu", "yanshi-9.9.9-aarch64-unknown-linux-gnu.deb"],
    ["x86_64-apple-darwin", "yanshi-9.9.9-x86_64-apple-darwin.dmg"],
    ["aarch64-apple-darwin", "yanshi-9.9.9-aarch64-apple-darwin.dmg"],
    ["x86_64-pc-windows-msvc", "yanshi-9.9.9-x86_64-pc-windows-msvc.msi"],
  ];
  for (const [target, want] of CASES) {
    let got = "";
    try {
      got = execFileSync("bash", [SCRIPT, "--print-name", "--target", target, "--version", "9.9.9"], {
        encoding: "utf8",
      }).trim();
    } catch (error) {
      failures.push(`--print-name --target ${target} 跑不起来：${error.message}`);
      continue;
    }
    if (got !== want) failures.push(`--print-name ${target} 应为 ${want}，实得 ${got} ⇒ 命名契约漂移`);
  }
}

// ③④ 两条发布线的接线与计数口径。
for (const wf of [".github/workflows/release.yml", ".github/workflows/nightly.yml"]) {
  const path = join(REPO_ROOT, wf);
  if (!existsSync(path)) {
    failures.push(`${wf} 不存在`);
    continue;
  }
  const text = readFileSync(path, "utf8");
  if (!/package-installer\.sh\s+--archive/.test(text)) {
    failures.push(`${wf} 没有实际调用 scripts/package-installer.sh --archive ⇒ 这条线不会产出安装包`);
  }
  for (const ext of ["msi", "dmg", "deb"]) {
    if (!new RegExp(`\\*\\.${ext}\\b`).test(text)) {
      failures.push(`${wf} 的产物集合里没有 *.${ext} ⇒ 安装包打出来也传不上去`);
    }
  }
  if (!/n_installers/.test(text)) {
    failures.push(`${wf} 的校验和计数断言没有 n_installers ⇒ 条目数与「归档+安装包」对不上，发布会被自己的断言拦死`);
  }
}

if (failures.length > 0) {
  console.error(`❌ 安装包接线判据红：${failures.length} 条`);
  for (const f of failures) console.error(`   ${f}`);
  process.exit(1);
}
console.log("✓ 安装包：五目标命名契约一致，release 与 nightly 两条线都接了构建步、上传集合与计数断言");
