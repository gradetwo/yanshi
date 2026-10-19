// **★★★ 「这个文件该不该改」 ✗ ★★★**（**第 609 轮 ✓；用户要求「为易错工作造工具」 ✓）
//
// **∴ 它解决什么 ✗**：**本会话最痛的坑之一是**改错文件**** ✗：
//   **∴ 症状 ✗**：**改完 JS ⇒ `git commit` 报「无文件要提交，工作区干净」**
//     ＋ **∴ 而** python 报 ok ＋ `node --check` 通过 ⇒ **∴ 文件**确实改了** ✓
//       ⇒ **∴ 根因 ✗**：**`web/viewer-app.js` **被 `.gitignore` 忽略**（**它是**构建产物 ✓）
//         ＋ **∴ 而**内核 JS 的**源**在 `crates/yanshi-http/assets/viewer-app.js`** ✓
//           ⇒ **★ 所以：**改产物 ⇒ 不入库 ＋ 下次同步被覆盖**** ✓（**白干一轮 ✓）★
//
// **∴ 为什么再犯 ✗**：**目录里同名文件太多**（**`web/` 与 `crates/**/assets/` ✓）
//   ＋ **∴ 而**「产物 vs 源」的规则**只在人脑里** ✓ ⇒ **∴ 所以**：**做成一问一答的工具** ✓ ★
//
// **∴ 用法 ✗**：
//   node scripts/tool-which-source.mjs <路径>            # 判一个文件
//   node scripts/tool-which-source.mjs --self-test        # 自检（**不读仓库 ✓）
//
// **∴ 退出码 ✗**：0 ＝ 该改（源）；1 ＝ **不该改**（产物 ⇒ 已给出源头 ✓）；3 ＝ 用法／自检错
import { existsSync, readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";

// **∴ 通用规则 ✗**：`web/<名字>` 的源头在 `crates/yanshi-http/assets/<名字>` ✓
//   （**∴ 依据 ✗**：**`pwa-sync-viewer.mjs` 干的就是这件事** ✓）
const SRC_OF_WEB = (name) => `crates/yanshi-http/assets/${name}`;

function gitIgnored(p) {
  try {
    execFileSync("git", ["check-ignore", "-q", p], { stdio: "ignore", timeout: 90_000 });
    return true;
  } catch {
    return false;
  }
}
function gitTracked(p) {
  try {
    const out = execFileSync("git", ["ls-files", "--error-unmatch", p], { stdio: ["ignore", "pipe", "ignore"], timeout: 90_000 });
    return out.length > 0;
  } catch {
    return false;
  }
}
function gitignoreLine(p) {
  try {
    return execFileSync("git", ["check-ignore", "-v", p], { encoding: "utf8" }).trim();
  } catch {
    return null;
  }
}

export function judge(p) {
  const ignored = gitIgnored(p);
  const tracked = gitTracked(p);
  const line = ignored ? gitignoreLine(p) : null;
  const verdict = {
    path: p,
    ignored,
    tracked,
    gitignoreRule: line,
    exists: existsSync(p),
    source: null,
    advice: "",
  };
  if (ignored && p.startsWith("web/")) {
    const name = p.slice("web/".length);
    const cand = SRC_OF_WEB(name);
    verdict.source = existsSync(cand) ? cand : null;
    verdict.advice = verdict.source
      ? `★ 不要改这个（**它在 .gitignore 里 ＝ 构建产物**）⇒ 改源：${verdict.source} ⇒ 然后 npm run pwa:sync`
      : `★ 不要改这个（**被 .gitignore 忽略**）⇒ 它是产物；**找不到明显源头** ⇒ 先查 pwa-sync-*.mjs`;
  } else if (ignored) {
    verdict.advice = "★ 不要改这个（**被 .gitignore 忽略**）⇒ 它是产物";
  } else if (tracked) {
    verdict.advice = "✓ 可以改（**git 跟踪的源 ✓）";
  } else {
    verdict.advice = "⚠️ 既没被忽略、也没被跟踪 ⇒ **改它也不会入库** ⇒ 先 git add -N 或确认路径";
  }
  return verdict;
}

const argv = process.argv.slice(2);
if (argv.includes("--self-test")) {
  // **∴ 自检（**不读仓库 ✓）✗：**只验**两条纯规则** ✓
  const webish = "web/viewer-app.js";
  const v = { ignored: true, path: webish };
  const src = SRC_OF_WEB(webish.slice(4));
  const ok = src === "crates/yanshi-http/assets/viewer-app.js";
  console.log(ok ? "  ✓ 自检通过（web/X ⇒ crates/yanshi-http/assets/X ✓）" : "  ✗ 自检失败");
  process.exit(ok ? 0 : 3);
}
if (argv.length === 0) {
  console.error("  ⊘ 不可判：缺参数（判据前提不成立）｜用法：node scripts/tool-which-source.mjs <路径>｜--self-test");
  process.exit(3);
}
let bad = false;
for (const p of argv) {
  const v = judge(p);
  console.log(`── ${v.path} ──`);
  console.log(`  存在：${v.exists ? "是" : "否"}｜被 .gitignore 忽略：${v.ignored ? "是" : "否"}｜被 git 跟踪：${v.tracked ? "是" : "否"}`);
  if (v.gitignoreRule) console.log(`  规则：${v.gitignoreRule}`);
  console.log(`  ${v.advice}`);
  if (v.ignored) bad = true;
}
process.exit(bad ? 1 : 0);
