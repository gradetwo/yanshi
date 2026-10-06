#!/usr/bin/env node
// **仓库顶层洁净度判据**（防止"判据自己跑出来的产物"被提交进仓库）。
//
// 背景（真实事故）：本地批量跑判据时，有的脚本把截图/下载目录拼成
// `<64 位十六进制>/crit_<判据名>.png`，而那个哈希目录被写在**仓库根**下，
// 于是 `git add -A` 把两个**空白的 300×150 截图**提交进了仓库
// （提交 4c5abd4 / 4fbd436）。没有任何代码引用它们，纯垃圾，却永久留在历史里。
//
// 判据：仓库顶层的每个条目，必须**要么**在白名单里，**要么**被 .gitignore 覆盖。
// 两者都不是 ⇒ 红。这样"产物写进仓库根并被提交"会在门禁就被抓住。
// 被忽略的条目算允许，是因为"忽略"正是仓库表达"这是产物、不该进版本库"的方式。
import { readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { execFileSync } from "node:child_process";

// **根目录按脚本自身位置定位** ✗：`run-criteria.sh` 给 `tool-*` 传的第一个位置参数是
// **base URL**（例如 `http://127.0.0.1:13990`）✗ ⇒ 原先 `process.argv[2]` 把它当目录 ⇒
// `readdirSync("http://…")` 必然 `ENOENT` ✗（CI 实测：`scandir 'http://127.0.0.1:13990'` ✓）。
// ⇒ 忽略位置参数，只按脚本目录定位仓库根 ✓（与 `tool-repo-root-hex-dirs.mjs` 一致 ✓）。
const root = join(dirname(fileURLToPath(import.meta.url)), "..");

// 白名单：仓库允许存在于顶层的条目（新增顶层文件/目录时必须同时更新这里）。
const ALLOWED = new Set([
  ".github", ".gitignore",
  "AGENTS.md", "CONTRIBUTING.md", "LICENSE", "Makefile", "README.md", "README.zh-CN.md",
  "SECURITY.md", "Cargo.toml", "Cargo.lock", "rustfmt.toml", "rust-toolchain.toml",
  "assets", "crates", "deploy", "docs", "scripts", "target",
]);

const entries = readdirSync(root).filter((name) => name !== ".git");

const run = (args) => {
  try {
    return execFileSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
  } catch {
    return "";
  }
};

const tracked = new Set(
  run(["ls-files"]).split("\n").map((p) => p.split("/")[0]).filter(Boolean),
);

// 被 .gitignore 覆盖的顶层条目（check-ignore 在没有匹配时退出码非零，因此用 run 吞掉）。
const ignored = new Set(
  run(["check-ignore", "--no-index", ...entries])
    .split("\n")
    .map((line) => line.split("/")[0].trim())
    .filter(Boolean),
);

const unexpected = entries.filter((name) => !ALLOWED.has(name) && !ignored.has(name));

if (unexpected.length > 0) {
  console.error(`✗ 仓库顶层出现 ${unexpected.length} 个未预期条目：`);
  for (const name of unexpected) {
    const kind = tracked.has(name) ? "**已被 git 跟踪（更严重）**" : "未跟踪（但 git add -A 会吃掉它）";
    let info = "";
    try {
      const st = statSync(`${root}/${name}`);
      info = st.isDirectory() ? "目录" : `文件 ${st.size}B`;
    } catch {
      info = "（读不到）";
    }
    console.error(`   - ${name}｜${info}｜${kind}`);
  }
  console.error("⇒ 判据产物的正确去处：/var/tmp/… 或 target/…（都被 .gitignore 覆盖）。");
  console.error("⇒ 如果这是**有意的**新顶层条目，请把名字加进本脚本的 ALLOWED 白名单。");
  process.exit(1);
}

console.log(`结论：仓库顶层 ${entries.length} 个条目都在白名单内或被忽略 ✓（无判据产物残留 ✓）`);
