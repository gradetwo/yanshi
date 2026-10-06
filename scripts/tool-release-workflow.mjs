#!/usr/bin/env node
// **判据：发布工作流必须存在、两种触发都在、且用真实入口** ✓。
//
// **为什么** ✓（2026-10-06 用户要求"增加 GitHub Actions 自动发布打包（tag 触发和手工触发）" ✓）：
// 这类 YAML **没有任何东西在跑它** ✗ ⇒ 容易写成一个**永远不触发**或**调用了不存在的脚本**的文件，
// 而且要到**真的要发版**时才发现 ✗ ⇒ 所以用一条判据把它钉住 ✓。
//
// **判据四条** ✓：
//   ① `.github/workflows/release.yml` 存在 ✓；
//   ② 触发里**既有 tag**（`v*` ✓）**又有 `workflow_dispatch`** ✓；
//   ③ 调用的脚本**真实存在** ✓（`scripts/package-release.sh` 与 `scripts/release-verify-archive.sh` ✓）；
//   ④ **引用的是 Makefile 之外的真实路径** ✓：打包命令必须出现 `package-release.sh` ✓
//      （**∴ 防止有人写成 `make release` 之外的自造命令 ✗ 或写错脚本名 ✓**）。
//
// **能红** ✓：删掉 `workflow_dispatch:` 一行 ⇒ 立刻红 ✓；把 `package-release.sh` 改成别的名字 ⇒ 红 ✓。
import { readFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const wfRel = ".github/workflows/release.yml";
const wf = join(root, wfRel);
const failures = [];

if (!existsSync(wf)) {
  failures.push(`${wfRel} 不存在 ⇒ tag 与手工触发都没有落点`);
} else {
  const text = readFileSync(wf, "utf8");
  if (!/^\s{2}push:/m.test(text) || !/^\s{4}tags:/m.test(text)) {
    failures.push(`${wfRel} 的触发里没有 tag（缺 push.tags）⇒ 打 tag 不会发版`);
  }
  if (!/workflow_dispatch:/.test(text)) {
    failures.push(`${wfRel} 的触发里没有 workflow_dispatch ⇒ 手工发不了版`);
  }
  if (!/v\*"|'v\*'/.test(text)) {
    failures.push(`${wfRel} 的 tag 过滤器里没有 v* ⇒ 约定与 tag 名不一致`);
  }
  if (!/package-release\.sh/.test(text)) {
    failures.push(`${wfRel} 没有调用 scripts/package-release.sh ⇒ 不是真实的打包入口`);
  }
  // 引用的脚本必须真实存在 ✓（把 YAML 里出现的 `scripts/x.sh` 逐个核对 ✓）。
  for (const m of text.matchAll(/(scripts\/[A-Za-z0-9._-]+\.(?:sh|mjs))/g)) {
    const rel = m[1];
    if (!existsSync(join(root, rel))) failures.push(`${wfRel} 引用了不存在的脚本：${rel}`);
  }
  if (!/SHA256SUMS/.test(text)) {
    failures.push(`${wfRel} 没有处理 SHA256SUMS ⇒ 校验和可能不随包发布`);
  }
}

if (failures.length > 0) {
  console.error(`❌ 发布工作流判据红：${failures.length} 条`);
  for (const f of failures) console.error(`   ${f}`);
  process.exit(1);
}
console.log("✓ 发布工作流：两种触发都在、入口真实、校验和随包 ✓");
