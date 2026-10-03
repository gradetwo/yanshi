#!/usr/bin/env node
// **人类界面的文字密度**判据（用户原话：Web 界面文字密密麻麻、很多是约定俗成的东西、不必大段介绍）。
//
// 判据（能红）：查看器 HTML 里**直接可见**的文本节点，不得是"长解释句"——
//   长度 ≥ 24 个汉字，且含 `；` / `（` 这类**解释性标点** ⇒ 红 ✗（应当进 `title=` 提示或只在 MCP 侧说清）。
// 为什么这样定：人类界面靠**约定俗成的图标与短标签** ✓（专业软件都如此 ✓）；
// 长解释属于**工具描述 / tooltip**（agent 读得到 ✓，人不被干扰 ✓）。
import { readFileSync } from "node:fs";
const source = readFileSync("crates/yanshi-http/src/viewer.rs", "utf8");
// **带行号** ✗：原来只打文本 ⇒ 还得自己回去找位置 ✓；现在直接给"第几行" ✓，剩下就是机械活 ✓。
const offenders = [];
const lines = source.split("\n");
for (const match of source.matchAll(/>([^<>{}]{24,})</g)) {
  const text = match[1].trim();
  const cjk = (text.match(/[\u4e00-\u9fff]/g) || []).length;
  if (cjk < 24) continue;
  if (!/[；（]/.test(text)) continue;
  const line = source.slice(0, match.index).split("\n").length;
  offenders.push({ line, text });
}
if (offenders.length) {
  console.log("  ✗ 人类界面上还有 " + offenders.length + " 处大段解释（应进 title= 或只在 MCP 侧说清）：");
  for (const item of offenders.slice(0, 8)) {
    console.log("     - viewer.rs:" + item.line + "  " + item.text.slice(0, 56));
  }
  console.log("  结论：界面文字密度不合格 ✗");
  process.exit(1);
}
console.log("  ✓ 人类界面没有大段解释句（长说明都在 title= / 工具描述里）");
process.exit(0);
