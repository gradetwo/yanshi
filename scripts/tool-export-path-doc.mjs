#!/usr/bin/env node
// **`export_png` 必须把写盘约束写在描述里**判据（来源：第三方 MCP 报告 P1-3 ✓ ——
// 测试者的建议原话就是「或**文档里明确说明限制**」✓）。它踩的坑：拒绝写任意目录 ✗ 却**没在描述里说** ✓。
// 用法：node scripts/tool-export-path-doc.mjs <server-base> <doc-id> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) { console.error("用法: node scripts/tool-export-path-doc.mjs <base> <doc> <token>"); process.exit(2); }
const catalogue = await (await fetch(`${base}/api/tools?doc=${doc}&token=${token}`)).json();
const tool = (catalogue.tools || []).find((entry) => entry.name === "export_png");
if (!tool) { console.log("  ✗ 目录里没有 export_png"); process.exit(1); }
const text = JSON.stringify(tool);
const must = [
  ["导出目录", "说明缺省的导出目录白名单"],
  ["YANSHI_EXPORT_DIR", "给出改目录的正规出路"],
  ["cp", "告诉调用方要放别处就自己复制"],
];
let bad = 0;
for (const [needle, why] of must) {
  const ok = text.includes(needle);
  console.log(`  ${ok ? "✓" : "✗"} ${why}（找 ${needle}）`);
  if (!ok) bad += 1;
}
if (bad) { console.log(`  ✗ export_png 的描述没把写盘限制说清（缺 ${bad} 项）`); process.exit(1); }
console.log("  ✓ export_png 的描述写清了写盘沙箱与出路");
