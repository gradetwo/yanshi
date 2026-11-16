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
//   ⑤ DEB 的桌面身份对齐：脚本里 .desktop 文件名、StartupWMClass 与 Icon 名
//      同为一个标识（today.wangda.yanshi，与 macOS bundle id 同一个），且图标
//      按 hicolor 规范出 scalable SVG ＋ 48/64/128/256 PNG——三者不一致时
//      Wayland 启动器里图标与窗口对不上号，且没有任何构建错误会提示。
//   ⑥ 三处已实证的校验写法（2026-11-16 三轮同红的复盘，缺一不可）：
//      a. 两条线的 DEB 校验不许 `dpkg-deb --contents … | grep -q`——grep 匹配后
//         关管，tar 写 stdout 得 SIGPIPE 退 2，pipefail 下误报「文件缺失」
//         （包里其实有文件；清单必须先落盘再 grep 文件）；
//      b. DMG 冒烟后必须先按端口杀掉 yanshi-serve 进程树再卸载——只 kill 子 shell
//         的话孤儿服务器占着挂载卷，hdiutil detach 报 Resource busy（exit 16）；
//         且最终卸载必须带重试（detached=yes），不许裸 detach 一次失败就红；
//      c. 脚本里不许出现非注释行的 `sed -i `（macOS BSD sed 要求 -i 带后缀；
//         与 crates/yanshi-server/tests/script_portability.rs 同一条规则，
//         注释行不算——那条测试跳过 `#` 开头的行）。
//
// **变异**：把 nightly.yml 里 `package-installer.sh` 的调用删掉 ⇒ ③ 红；
//           把脚本里 linux 的扩展名改成 zip ⇒ ② 红；
//           把某条线的 `dpkg-deb --contents … > "$contents"` 改回管道 grep ⇒ ⑥a 红；
//           删 `pkill -f "yanshi-serve` 行或 detach 重试 ⇒ ⑥b 红；
//           把脚本里 `sed … > tmp && mv` 改回 `sed -i` ⇒ ⑥c 红。
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

  // ⑤ 桌面身份对齐（读脚本原文）。
  const scriptText = readFileSync(SCRIPT, "utf8");
  if (!/app_id="today\.wangda\.yanshi"/.test(scriptText)) {
    failures.push("package-installer.sh 里没有 app_id=\"today.wangda.yanshi\" ⇒ .desktop 身份标识漂移");
  }
  if (!/StartupWMClass=\$\{app_id\}/.test(scriptText)) {
    failures.push("package-installer.sh 的 .desktop 没有 StartupWMClass=${app_id} ⇒ 与文件名/Icon 名对不齐");
  }
  if (!/scalable\/apps/.test(scriptText)) {
    failures.push("package-installer.sh 没有往 hicolor/scalable/apps 装 SVG 图标");
  }
  if (!/for size in 48 64 128 256/.test(scriptText)) {
    failures.push("package-installer.sh 的 PNG 图标尺寸档不是 48 64 128 256 ⇒ hicolor 图标集不全");
  }

  // ⑥c sed -i（只查非注释行，与 script_portability.rs 同口径）。
  const badSed = scriptText
    .split("\n")
    .map((line, i) => [i + 1, line])
    .filter(([_, line]) => !line.trimStart().startsWith("#") && line.includes("sed -i "));
  for (const [lineno] of badSed) {
    failures.push(`package-installer.sh:${lineno} 有非注释行的 \`sed -i \` ⇒ macOS BSD sed 不接受无后缀 -i`);
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

  // ⑥a DEB 清单不许走管道 grep（pipefail + SIGPIPE 假红；注释行不算——
  //    解释文字里会逐字提到这个写法）。
  const pipedGrep = text
    .split("\n")
    .some((line) => !line.trimStart().startsWith("#") && /dpkg-deb --contents[^|]*\|\s*grep/.test(line));
  if (pipedGrep) {
    failures.push(`${wf} 的 DEB 校验还在用 \`dpkg-deb --contents … | grep -q\` ⇒ pipefail 下 grep 关管致 tar SIGPIPE 假红，清单须先落盘再 grep 文件`);
  }
  // ⑥b DMG 冒烟后先杀服务器进程树再卸载，且卸载带重试。
  if (!/pkill -f "yanshi-serve/.test(text)) {
    failures.push(`${wf} 的 DMG 校验没有按端口杀 yanshi-serve 进程树 ⇒ 孤儿服务器占卷，hdiutil detach 报 Resource busy`);
  }
  if (!/detached=yes/.test(text)) {
    failures.push(`${wf} 的 DMG 校验没有卸载重试（detached=yes）⇒ 服务器退出滞后时裸 detach 一次失败就红`);
  }
}

if (failures.length > 0) {
  console.error(`❌ 安装包接线判据红：${failures.length} 条`);
  for (const f of failures) console.error(`   ${f}`);
  process.exit(1);
}
console.log("✓ 安装包：五目标命名契约一致，release 与 nightly 两条线都接了构建步、上传集合与计数断言");
