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
//   **⇒ 豁免**：`gh api` 调用且其后第一个位置参数是显式 API 路径（以 repos/、orgs/、user/、search/ 等
//   API 根开头，或 https://api.github.com/ 全址）时不计违规 —— 仓库已在 URL 里，`gh` 不从 `.git` 推断 ✓
//   （`gh release`／`gh run` 等需仓库解析的子命令仍严格要求 --repo／-R，不豁免 ✓）
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

// **⇒ 纯函数** ✅（第 1061 轮抽出前两个，能**自测** ✓；豁免为本次新增）：
//   ⇒ 「gh 命令」= 含 gh 词、且不是注释 ✓
//   ⇒ 「有 --repo」= 命令行里出现 --repo 或 -R ✓
//   ⇒ 「gh api 豁免」= `gh api` 后的第一个位置参数是显式 API 路径（见上豁免）✓
//   ⇒ 「合规」= 不是 gh 命令，或带 --repo，或属 gh api 豁免 ✓（扫描与自测共用同一谓词）
function isGhCommand(line) {
  if (/^\s*#/.test(line)) return false;
  return /(^|\s)gh\s/.test(line);
}
function hasRepoFlag(line) {
  return /--repo\s|-R\s/.test(line);
}
// 切出 `gh api` 后的第一个位置参数：跳过 -X/--method 等带值开关与无值开关，
// 去引号、去前导 `/`、去 api.github.com 全址前缀后，判是否以 API 根开头。
const API_ROOTS = /^(repos|orgs|users|user|search|gists|emojis|events|feeds|meta|notifications|rate_limit|markdown|licenses|gitignore)\//;
function isExemptGhApiCall(line) {
  const m = /(^|\s)gh\s+api\s+(.*)$/.exec(line);
  if (!m) return false;
  const rest = m[2];
  // 按引号切 token（单引号／双引号内空格不切）。
  const tokens = [];
  let cur = "";
  let quote = null;
  for (const ch of rest) {
    if (quote) {
      cur += ch;
      if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
      cur += ch;
    } else if (/\s/.test(ch)) {
      if (cur) { tokens.push(cur); cur = ""; }
    } else {
      cur += ch;
    }
  }
  if (cur) tokens.push(cur);
  const valueFlags = new Set(["-X", "--method", "-H", "--header", "-f", "--raw-field", "-F", "--field", "--jq", "-o", "--output", "--input"]);
  for (let i = 0; i < tokens.length; i += 1) {
    const t = tokens[i];
    if (valueFlags.has(t)) { i += 1; continue; }
    if (t.startsWith("-")) continue;
    let p = t.replace(/^["']|["']$/g, "");
    p = p.replace(/^\/+/, "");
    p = p.replace(/^https:\/\/api\.github\.com\//, "");
    return API_ROOTS.test(p);
  }
  return false;
}
function isCompliantGhCommand(line) {
  return !isGhCommand(line) || hasRepoFlag(line) || isExemptGhApiCall(line);
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
      if (!isCompliantGhCommand(line)) {
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
console.log(`  扫描 ${files.length} 个工作流｜无 checkout 的作业 ${jobsChecked} 个｜其中「gh」命令 ${ghCommands} 条，全部合规（显式 --repo 或 gh api 全路径豁免）✓`);
console.log("✓ 无 checkout 的作业里，`gh` 命令都合规（显式指定仓库，或 gh api 全路径调用；结构检查：只证明命令形态）");

// **⇒ 自测** ✅（第 1061 轮）：六段命令行让合规谓词**自己能红** ✓（不必改工作流 ✓）
//   **⇒ 变异**：把 `hasRepoFlag` 恒返回 `true` ⇒ 不带 --repo 的那两段变红 ✓
//   **⇒ 变异**：把 `isExemptGhApiCall` 恒返回 `false` ⇒ 两条 gh api 全路径段变红 ✓
if (process.argv.includes("--self-test")) {
  const cases = [
    ["带 --repo 的命令", "          gh release upload v1 f --repo o/r", true],
    ["不带 --repo 的命令", "          gh release upload v1 f", false],
    ["注释里的 gh", "  #  gh release upload v1 f", true],
    ["gh api 全路径调用", '          gh api "repos/${GITHUB_REPOSITORY}/releases/tags/nightly" | jq .', true],
    ["gh api 带 -X 的全路径调用", '          gh api -X DELETE "repos/${GITHUB_REPOSITORY}/releases/assets/${aid}" --silent', true],
    ["gh api 非全路径仍违规", "          gh api repos --jq .", false],
  ];
  let pass = 0;
  for (const [label, line, wantOk] of cases) {
    const isGh = isGhCommand(line);
    const ok = isCompliantGhCommand(line);
    console.log("  " + (ok === wantOk ? "✓" : "✗") + " " + label + "：gh=" + isGh + " ok=" + ok);
    if (ok === wantOk) pass += 1;
  }
  console.log("自测：" + pass + "/" + cases.length + " 通过");
  process.exit(pass === cases.length ? 0 : 1);
}

