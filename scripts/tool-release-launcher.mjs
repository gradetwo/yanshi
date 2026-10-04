#!/usr/bin/env node
// **发布启动脚本的开关判据**（第 772 轮）：`scripts/package-release.sh` 在打包的最后
// 生成一个 `yanshi.sh`，它把一串开关交给 `yanshi-serve`。若其中任何一个**二进制不认识**，
// 打包后的启动脚本一跑就退出（clap 会直接报「未知参数」）✗。
//
// 为什么需要它 ✓：全仓 73 条判据里**没有一条覆盖发布包** ✗ ⇒ 僵死开关可以长期留在树里，
// 而**所有判据照样全绿**。第 772 轮实测就是这样：`--no-brush-wasm` 已随第二份笔刷实现
// 一起从产品里删除，而启动脚本仍在传它，且**无条件**传（它测的那个文件永远不存在）✗。
//
// 判据（都能红）：
//   ① 从启动脚本的 `exec` 块里收集 `--xxx` 开关 —— 只看**非注释**行（注释里会提到历史开关 ✓）；
//   ② 每个开关都**真的交给二进制解析一次**，看它是否回「未知参数」；
//   ③ 二进制缺失 ⇒ 明确报「无法运行」并**非零退出**（不是"跳过并通过" ✗）。
//
// ⚠️ 不能用 `<binary> <flag> --help` 探测：`--help` 会**短路**（不看别的参数就打印帮助并 exit 0 ✗），
//    所以那样连已删的开关都"通过" ✓ —— 这是本轮亲手证伪过的做法。
//
// 用法：node scripts/tool-release-launcher.mjs [base-url]
//      （base-url 不使用，只为与 `tool-*.mjs` 的约定一致 ⇒ 命令行参数可省 ✓）

import { readFileSync, existsSync, mkdtempSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

const LAUNCHER = "scripts/package-release.sh";
const BINARY = "target/debug/yanshi-serve";

if (!existsSync(BINARY)) {
  console.error("❌ 发布启动脚本判据无法运行：找不到 " + BINARY + "（先跑 cargo build）");
  process.exit(1);
}

const src = readFileSync(LAUNCHER, "utf8").split("\n");
const start = src.findIndex((line) => line.includes('exec "$here/bin/yanshi-serve"'));
if (start < 0) {
  console.error("❌ 发布启动脚本判据无法运行：在 " + LAUNCHER + " 里找不到 exec 行");
  process.exit(1);
}
let end = start;
while (end < src.length && !src[end].includes('"$@"')) end += 1;

const block = src.slice(start, end + 1).filter((line) => !line.trim().startsWith("#"));
const flags = new Set();
for (const line of block) for (const found of line.matchAll(/--[a-z][a-z0-9-]+/g)) flags.add(found[0]);

if (flags.size < 2) {
  console.error("❌ 发布启动脚本判据无法运行：只从 exec 块里读到 " + flags.size + " 个开关，疑似解析失败");
  process.exit(1);
}

const work = mkdtempSync(join(tmpdir(), "yanshi-launcher-"));
const valueFor = ["--root", work, "--doc", "boot"];

/** 把某个开关交给二进制解析 ⇒ 回 { accepted, message }。 */
function probe(flag) {
  const runs = [[flag], [flag, "/tmp"]];
  for (const args of runs) {
    try {
      const out = execFileSync(BINARY, args.concat(valueFor), {
        encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: 4000,
      });
      const text = String(out);
      if (/未知参数|unexpected argument|unrecognized/i.test(text)) return { accepted: false, message: text.trim().split("\n")[0] };
      return { accepted: true, message: "启动到运行期（开关被接受）" };
    } catch (err) {
      const text = String((err && (err.stderr || err.stdout)) || err.message || err);
      if (/未知参数|unexpected argument|unrecognized/i.test(text)) return { accepted: false, message: text.trim().split("\n")[0] };
      if (/需要值|requires a value|value is required/i.test(text)) continue;
      return { accepted: true, message: "启动到运行期（开关被接受）" };
    }
  }
  return { accepted: true, message: "两种形式都未被判为未知（按接受处理）" };
}

console.log("  启动脚本传的开关：" + [...flags].join(" "));
let bad = 0;
for (const flag of [...flags].sort()) {
  const verdict = probe(flag);
  if (verdict.accepted) console.log("  ✓ " + flag + " ⇒ " + verdict.message);
  else {
    bad += 1;
    console.error("  ✗ " + flag + " ⇒ 二进制不认识它：" + verdict.message);
  }
}
if (bad > 0) {
  console.error("❌ 发布启动脚本传了 " + bad + " 个二进制不认识的开关 ✗ ⇒ 打包后的 yanshi.sh 一跑就退出");
  process.exit(1);
}
console.log("✓ 发布启动脚本的开关全部被二进制接受（" + flags.size + " 个）");
