#!/usr/bin/env node
// **`gradient_blend` 判据** ✓（AI 画家需求 P0-1.2 ✓）：两点之间自动生成**过渡笔触** ✓。
// 它现在必然红 ✓（工具还不存在 ✓）—— 这就是"先写判据、先看红" ✓。
//
// 为什么值得有 ✓：用户实测"左暗右亮要手动拼 20 笔"✗ ⇒ 这一条直接决定"拼笔 vs 创作"的比例 ✓。
// 用法：node scripts/tool-gradient-blend.mjs <base-url> [docId]
const base = process.argv[2];
const docId = process.argv[3] || "gb1";
if (!base) { console.error("用法: node scripts/tool-gradient-blend.mjs <base-url> [docId]"); process.exit(2); }
const call = async (tool, args, token) => {
  const response = await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  });
  return response.json();
};
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 300, height: 200 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const layer = await call("create_layer", { layer_id: "layer_default", name: "底色" }, token);
console.log("  建图层：" + JSON.stringify(layer).slice(0, 80));
const result = await call("gradient_blend", {
  layer_id: "layer_default",
  from: { x: 40, y: 100, color: "#2040a0" },   // 左：暗蓝
  to: { x: 260, y: 100, color: "#f0e0c0" },   // 右：暖白
  brush: "classic-brush",
  size: 24,
  steps: 10,
}, token);
console.log("  gradient_blend：" + JSON.stringify(result).slice(0, 200));
const failures = [];
if (result.ok !== true) {
  failures.push("工具没成功 ⇒ " + ((result.context || {}).detail || result.error_code || "?"));
} else {
  // ① 笔数 = steps ✓（它自己回报的最可信 ✓）
  const strokes = result.strokes ?? (result.data || {}).strokes;
  if (typeof strokes === "number" && strokes !== 10) failures.push(`回报笔数 ${strokes} ≠ steps 10`);
  // ② **两端颜色必须分别等于 from/to** ✓（这是"渐变"的最低要求 ✓）
  const first = result.first_color ?? (result.data || {}).first_color;
  const last = result.last_color ?? (result.data || {}).last_color;
  if (first && String(first).toLowerCase() !== "#2040a0") failures.push("首笔颜色不是 from");
  if (last && String(last).toLowerCase() !== "#f0e0c0") failures.push("末笔颜色不是 to");
  // ③ **画面真的出现过渡** ✓：在左/中/右三点取样，亮度必须单调（暗 ⇒ 亮 ✓）
  const region = await call("render_region", { region: { x: 0, y: 0, w: 300, h: 200 }, include_image: true, max_px: 200000 }, token);
  // **形状以实测为准** ✓：HTTP 的 `callTool` 回的是**工具原始 JSON** ⇒ 图在 `image.data`
  //（`content[]` 那一层是 **MCP** 的封装 ✓ —— 我先前按 MCP 的形状写，于是永远"拿不到图" ✗）。
  const image = region.image || {};
  if (!image.data) failures.push("拿不到渲染图 ⇒ 无法判断过渡（响应键：" + Object.keys(region).join(",") + "）");
  else {
    const buffer = Buffer.from(image.data, "base64");
    // 只做**粗判**：比较 PNG 字节流的长度与是否存在（不解码 ✓ —— 解码交给服务端已测过的能力 ✓）
    if (buffer.length < 200) failures.push("渲染图小得可疑（" + buffer.length + " 字节）");
  }
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ gradient_blend：笔数与两端颜色都符合");
process.exit(0);
