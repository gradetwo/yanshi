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
// **导出目录优先放在工作区下** ✓（第 336 轮）：服务端只能写它自己**有权限的路径** ✗，
// 而 CI 里工作目录与这里不一致 ✗ ⇒ `export_png` 会**直接失败** ✓（实测：判据在 mjs:44 抛错 ✓）。
// 读回也用同一个目录 ✓ ⇒ **一处修，两边都跟着对** ✓。
const exportDir = process.env.YANSHI_WORKSPACE ? `${process.env.YANSHI_WORKSPACE}/exports` : (process.argv[3] || "exports").replace(/\/$/, "");;
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
  // **按候选路径查找** ✓（第 331 轮）：服务端把导出写在自己的 `--root` 下 ✓，
  // 而判据在仓库根下跑 ✗ ⇒ 只用一个相对路径在 CI 里必然 `ENOENT` ✗（实测 ✓）。
  // 候选里优先 `YANSHI_WORKSPACE`（**runner 已经导出它** ✓，`tool-batch-preview` 就是这么用的 ✓）。
  const candidates = (() => {
    const ws = process.env.YANSHI_WORKSPACE;
    const dirs = [process.env.YANSHI_EXPORT_DIR, ws && `${ws}/exports`, ws, exportDir, "."].filter(Boolean);
    return dirs.map((dir) => `${dir}/${docId}.png`);
  })();
  for (const candidate of candidates) {
    try {
      return createHash("sha256").update(readFileSync(candidate)).digest("hex");
    } catch (_) { /* 试下一个 ✓ */ }
  }
  // **失败时把试过的路径打出来** ✓ —— 下次它自己就能说明"我找过哪里" ✓。
  throw new Error(`找不到导出文件 ${docId}.png ⇒ 试过：${candidates.join(" ｜ ")}`);
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
