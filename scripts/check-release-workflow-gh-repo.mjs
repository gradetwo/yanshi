#!/usr/bin/env node
// **⇒ 工作流里 `gh` 命令在「没有 checkout 的作业」中必须显式给 `--repo`** ✅（第 1046 轮）
//
// **⇒ 为什么** ✗：v0.3.0 的发布实测 —— `checksums` 作业**只下载产物、没 checkout** ✓
//   ⇒ `gh release upload "$TAG" SHA256SUMS --clobber` ⇒ `failed to run git: not a git repository` ✗
//   ⇒ 而**自检那一步是过的**（`条目 4 ｜归档 4` ✓）⇒ 失败只在**最后一条命令** ✓
//   ⇒ 于是四个平台的包都打出来了，**用户却拿不到汇总的 SHA256SUMS** ✗
//
// **⇒ 判据**：对每个工作流文件、每个作业：
//   若该作业**没有** `actions/checkout` ⇒ 里面每条含 `gh ` 的命令行**必须**带 `--repo` 或 `-R `
//   （有 checkout 的作业不强制 ✓ —— 那时 `gh` 能从 `.git` 推断 ✓）
//
// **⇒ 变异**（本机可复核 ✓）：把 `release.yml` 的 `checksums` 作业里 `--repo "$GITHUB_REPOSITORY"` 删掉
//   ⇒ 本判据**红**并点名该作业 ✓
//
// **⇒ 它证明什么**（如实 ✓）：只证明**命令形态合规** ✓ 不证明运行时真能上传 ✓
import { readFileSync, readdirSync } from "node:fs";

const dir = ".github/workflows";
let files = [];
try {
  files = readdirSync(dir).filter((f) => /\.ya?ml$/.test(f));
} catch {
  console.error(`  ⊘ 找不到 ${dir} ⇒ 判据无法运行（不是通过）`);
  process.exit(3);
}
if (files.length === 0) {
  console.error("  ⊘ 工作流目录是空的 ⇒ 判据无法运行（不是通过）");
  process.exit(3);
}

const offenders = [];
let jobsChecked = 0;
let ghCommands = 0;
for (const file of files) {
  const lines = readFileSync(`${dir}/${file}`, "utf8").split("\n");
  // 作业起点：两格缩进、以 `名字:` 结尾
  const starts = [];
  lines.forEach((line, i) => {
    if (/^  [A-Za-z0-9_-]+:\s*$/.test(line)) starts.push(i);
  });
  for (let s = 0; s < starts.length; s += 1) {
    const from = starts[s];
    const to = s + 1 < starts.length ? starts[s + 1] : lines.length;
    const body = lines.slice(from, to);
    const name = lines[from].trim().replace(/:$/, "");
    const hasCheckout = body.some((l) => /uses:\s*actions\/checkout/.test(l));
    if (hasCheckout) continue;
    jobsChecked += 1;
    body.forEach((line, k) => {
      if (!/(^|\s)gh\s/.test(line)) return;
      if (/^\s*#/.test(line)) return; // 注释不算 ✓
      ghCommands += 1;
      if (!/--repo\s|-R\s/.test(line)) {
        offenders.push(`${file} › ${name}（第 ${from + k + 1} 行）`);
      }
    });
  }
}

for (const spot of offenders) {
  console.error(`  ✗ ${spot}：这个作业**没有 checkout**，「gh」命令却**没给 「--repo」** ⇒ 运行时会去当前目录找 .git 🔴`);
}
if (offenders.length > 0) {
  console.error(`  ✗ ${offenders.length} 处 \`gh\` 命令在无 checkout 的作业里缺少 \`--repo\``);
  process.exit(1);
}
console.log(`  扫描 ${files.length} 个工作流｜无 checkout 的作业 ${jobsChecked} 个｜其中「gh」命令 ${ghCommands} 条，全部显式给了 \`--repo\` ✓`);
console.log("✓ 无 checkout 的作业里，`gh` 命令都显式指定了仓库（结构检查：只证明命令形态）");
