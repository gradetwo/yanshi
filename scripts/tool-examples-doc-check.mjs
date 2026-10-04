#!/usr/bin/env node
// **`docs/design/tool-examples.md` 的防陈旧判据** ✓（第 389-390 轮 ✓）。
//
// **它补的是什么** ✓：生成器 `scripts/tool-examples-doc.mjs` 一直提供 `--check`（过期就 exit 1 ✓），
// **而仓库里没有任何东西调用它** ✗ —— 实测：`grep -rn "tool-examples-doc"` 在 workflows / scripts /
// Rust / manifests 里**零命中** ✓ ⇒ ⇒ **文档已经陈旧（47 小节 vs 声称 52 ✓）而无人发现** ✗ ✓。
// 更糟的是它自己在开头写着"**所以它不可能与实现漂移**" ✗ —— **一句背后没有检查的保证** ✓
//（与上一轮删掉的"全部经过实调验证"同族 ✗）。
//
// **本判据做的事** ✓：**转发调用生成器的 `--check`** ✓，**把它的退出码原样带出** ✓。
// **不重写它的逻辑** ✓ —— 否则就是"第二份实现" ✗（这个仓库在羽化那一课上学过 ✓）。
import { spawnSync } from "node:child_process";
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-examples-doc-check.mjs <server-base>"); process.exit(2); }
const run = spawnSync("node", ["scripts/tool-examples-doc.mjs", base, "--check"], {
  encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
});
const out = `${run.stdout || ""}${run.stderr || ""}`.trim();
if (out) console.log(out.split("\n").slice(-6).map((l) => `     ${l}`).join("\n"));
// **退出码原样带出** ✓：0 ⇒ 文档与目录一致 ✓；1 ⇒ 过期 ✗；2 ⇒ 用法错（判据无法运行 ✓ 也是红 ✓）。
process.exit(run.status === 0 ? 0 : 1);
