#!/usr/bin/env node
// **⇒ 工作流里 `gh` 命令在「没有 checkout 的作业」中必须显式给 `--repo`** ✅（第 1046 轮）
//
// **⇒ 为什么** ✗：v0.3.0 的发布实测 —— `checksums` 作业**只下载产物、没 checkout** ✓
//   ⇒ `gh release upload "$TAG" SHA256SUMS --clobber` ⇒ `failed to run git: not a git repository` ✗
//   ⇒ 而**自检那一步是过的**（`条目 4 ｜归档 4` ✓）⇒ 失败只在**最后一条命令** ✓
//   ⇒ 于是四个平台的包都打出来了，**用户却拿不到汇总的 SHA256SUMS** ✗
//
// **⇒ 判据**：对每个工作流文件、每个作业：
//   若该作业**没有** `actions/checkout` ⇒ 里面每条含 `gh ` 的命令行**必须**自带仓库上下文：
//   `--repo`／`-R`，**或者**是 `gh api` 且端点写成完整路径 `repos/<owner>/<repo>/…`
//   （`gh api` **没有** `--repo` 选项 —— `gh api --help` 的 FLAGS 里没有它；
//    端点本身带 owner/repo ⇒ 与 `--repo` 同样不需要 `.git`）。
//
// **⇒ 变异**（本机可复核 ✓）：把 `release.yml` 的 `checksums` 作业里 `--repo "$GITHUB_REPOSITORY"` 删掉
//   ⇒ 本判据**红**并点名该作业 ✓
//   （`--self-test` 也验：`gh api repos/o/r/x` 必须判合规、裸 `gh release upload` 必须判不合规 ✓）
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

// **⇒ 两个纯函数** ✅（第 1061 轮）：抽出来后能**自测** ✓（不必改工作流 ✓）
//   ⇒ 「gh 命令」= 含 gh 词、且不是注释 ✓
//   ⇒ 「有 --repo」= 命令行里出现 --repo 或 -R ✓
function isGhCommand(line) {
  if (/^\s*#/.test(line)) return false;
  return /(^|\s)gh\s/.test(line);
}
function hasRepoFlag(line) {
  if (/--repo\s|-R\s/.test(line)) return true;
  // `gh api` 没有 `--repo` 选项（FLAGS 实测），但端点自带 owner/repo 时
  // 与 `--repo` 等价：`gh api repos/<owner>/<repo>/…`。
  return /(^|\s)gh\s+api\s/.test(line) && /["']?\/?repos\//.test(line);
}

// **⇒ 自测先于扫描** ✅：谓词是自测的对象 ✓；扫描是**用**谓词 ✓
//   ⇒ 旧版把自测放在扫描之后 ⇒ 仓库里有违规时进程在扫描处 `exit(1)` ⇒
//     **自测根本没跑**，而 pre-commit 把这次失败报成「自测失败（谓词被改坏了？）」
//     ⇒ 句柄对不上对象（真因是 nightly.yml 的 `gh api`，不是谓词坏了）。
if (process.argv.includes("--self-test")) {
  const cases = [
    ["带 --repo 的命令", "          gh release upload v1 f --repo o/r", true],
    ["不带 --repo 的命令", "          gh release upload v1 f", false],
    ["注释里的 gh", "  #  gh release upload v1 f", true],
    ["gh api 带完整 repos/ 路径", "          gh api repos/o/r/releases/tags/nightly", true],
    ['gh api 拼 "repos/${GITHUB_REPOSITORY}/…"', '          gh api "repos/${GITHUB_REPOSITORY}/releases/tags/nightly"', true],
    ["gh api 只给相对端点", "          gh api releases/tags/nightly", false],
  ];
  let pass = 0;
  for (const [label, line, wantOk] of cases) {
    const isGh = isGhCommand(line);
    const ok = !isGh || hasRepoFlag(line);
    console.log("  " + (ok === wantOk ? "✓" : "✗") + " " + label + "：gh=" + isGh + " ok=" + ok);
    if (ok === wantOk) pass += 1;
  }
  console.log("自测：" + pass + "/" + cases.length + " 通过");
  process.exit(pass === cases.length ? 0 : 1);
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
      if (!isGhCommand(line)) return;
      ghCommands += 1;
      if (!hasRepoFlag(line)) {
        offenders.push({ file, job: name, line: from + k + 1 });
      }
    });
  }
}

for (const spot of offenders) {
  const label = `${spot.file} › ${spot.job}（第 ${spot.line} 行）`;
  console.error(`  ✗ ${label}：这个作业**没有 checkout**，「gh」命令却**没给 「--repo」** ⇒ 运行时会去当前目录找 .git 🔴`);
}
if (offenders.length > 0) {
  console.error(`  ✗ ${offenders.length} 处 \`gh\` 命令在无 checkout 的作业里缺少 \`--repo\``);
  process.exit(1);
}
console.log(`  扫描 ${files.length} 个工作流｜无 checkout 的作业 ${jobsChecked} 个｜其中「gh」命令 ${ghCommands} 条，全部显式给了 \`--repo\` ✓`);
console.log("✓ 无 checkout 的作业里，`gh` 命令都显式指定了仓库（结构检查：只证明命令形态）");

