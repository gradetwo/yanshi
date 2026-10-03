#!/usr/bin/env node
// **中文文字是否真被渲染**判据（第三方代码审计 P0 第 3 条：内置字体只有 5×7 ASCII ⇒ CJK 回退成 `?`）。
//
// 判法（**能红** 且不靠"看" ✓）：同一位置分别画「你好世界」与「????????」，各导出 PNG：
//   * 若两者**逐字节相同** ⇒ 中文确实被渲染成问号 ✗（判据红 ✓，这就是审计说的硬伤 ✓）；
//   * 若不同 ⇒ 中文有自己的字形 ✓（判据绿 ✓）。
// 用法：node scripts/tool-cjk-text.mjs <server-base> [exportDir]
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
const base = process.argv[2];
const exportDir = (process.argv[3] || "exports").replace(/\/$/, "");
if (!base) { console.error("用法: node scripts/tool-cjk-text.mjs <server-base> [exportDir]"); process.exit(2); }
const newDoc = async (id) => {
  const made = await fetch(`${base}/api/documents`, { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 260, height: 80 }) }).then((r) => r.json());
  if (!made.token) throw new Error("拿不到 token ⇒ 判据无法运行（不是通过）");
  return made.token;
};
const draw = async (docId, text) => {
  const token = await newDoc(docId);
  const tool = (name, args) => fetch(`${base}/api/tools?doc=${docId}&token=${token}`, { method: "POST",
    headers: { "content-type": "application/json" }, body: JSON.stringify({ tool: name, arguments: args }) }).then((r) => r.json());
  await tool("create_layer", { layer_id: "L1", name: "L1" });
  const drawn = await tool("draw_text", { layer_id: "L1", data: { x: 10, y: 20, text, size: 24,
    color: { r: 0, g: 0, b: 0, a: 255 } } });
  const out = await tool("export_png", { path: `${docId}.png` });
  if (drawn.ok !== true || out.ok !== true) {
    throw new Error(`画/导出失败 ⇒ 判据无法运行（不是通过）：${JSON.stringify({ drawn, out }).slice(0, 200)}`);
  }
  return createHash("sha256").update(readFileSync(`${exportDir}/${docId}.png`)).digest("hex");
};
const cjk = await draw("cjk-doc", "你好世界");
const marks = await draw("marks-doc", "????????");
const same = cjk === marks;
console.log(`  中文那幅的指纹：${cjk.slice(0, 16)}…｜问号那幅：${marks.slice(0, 16)}…`);
if (same) {
  console.log("  ✗ 「你好世界」与「????????」导出**完全相同** ⇒ 中文确实被渲染成问号（审计属实 ✗）");
  process.exit(1);
}
console.log("  ✓ 中文有自己的字形（与问号不同）");
process.exit(0);
