#!/usr/bin/env node
// **颜色写法必须写在 Schema 里**判据（第三方代码审计：颜色输入存在"暗色被放大 255 倍"的歧义）。
//
// 事实（读代码确认）：`[r,g,b,a]` **任一分量 > 1** ⇒ 按 0..255 字节 ✓；否则按 0..1 线性浮点 ✓。
// 歧义本身消除不掉（无类型数组的固有性质 ✓），**能消除的是"没说清"** ✗ ⇒ 判据即：
// 目录里每个颜色参数的**描述**都必须写明这条切换规则 ✓（否则 AI 会照着错理解写 ✗）。
// 用法：node scripts/tool-color-schema.mjs <server-base>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-color-schema.mjs <server-base>"); process.exit(2); }
const made = await fetch(base + "/api/documents", { method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: "colourschema", width: 100, height: 80 }) }).then((r) => r.json());
if (!made.token) { console.error("拿不到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const catalogue = await fetch(`${base}/api/tools?doc=colourschema&token=${made.token}`).then((r) => r.json());
const offenders = [];
let checked = 0;
for (const tool of catalogue.tools || []) {
  for (const param of (tool.inputSchema && tool.inputSchema.properties ? Object.entries(tool.inputSchema.properties) : [])) {
    const [name, schema] = param;
    if (!/colou?r/.test(name)) continue;
    checked += 1;
    const text = String(schema.description || "") + " " + String(tool.description || "");
    const statesRule = />\s*1|大于 1|>1/.test(text);
    // **"委托"也算说清** ✓：像 `color_to` / `medium_stroke.color` 写的"写法同 color" ✓，
    // 指向的那个参数自己写着规则 ✓ ⇒ 不必每处都抄一遍 ✗（抄了反而会漂移 ✗）。
    const delegates = /写法同|同 brush_stroke 的 color|同 color/.test(text);
    if (!statesRule && !delegates) offenders.push(`${tool.name}.${name}：描述没写「任一分量 > 1 即按字节」也没指向写了的地方`);
  }
}
console.log(`  检查了 ${checked} 个颜色参数`);
if (offenders.length) {
  for (const item of offenders.slice(0, 6)) console.log("     ✗ " + item);
  console.log("  结论：颜色写法在 Schema 里没说清 ✗");
  process.exit(1);
}
console.log("  ✓ 每个颜色参数都写明了「任一分量 > 1 即按字节」这条切换规则");
process.exit(0);
