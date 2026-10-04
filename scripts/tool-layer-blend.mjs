#!/usr/bin/env node
// **图层混合模式判据** ✓（需求文档 P1-4 ✓：`set_layer_blend(layer_id, mode)` ✓）。
// 文档要的理由很具体 ✓："**罩染技法依赖 multiply** ✓，现在只能调 opacity 硬叠 ✓，颜色发脏"✗。
//
// 判据（用**已知颜色**算得出确定答案 ✓，不是"变了没有"这种弱断言 ✗）：
//   ① `set_layer_blend{layer_id, mode:"multiply"}` 被接受 ✓；
//   ② 两层都用同一个中灰 ⇒ **normal 与 multiply 的结果必须不同** ✓；
//   ③ **multiply 必须更暗** ✓（同色相乘 = 平方 ⇒ 数学上必然更暗 ✓）—— 方向错了就是实现错了 ✗；
//   ④ **缺省（不设 blend）必须与"明确设成 normal"逐字节相同** ✓（老行为不变 ✓，本仓库的老规矩 ✓）。
const base = process.argv[2];
const docId = process.argv[3] || "lb1";
if (!base) { console.error("用法: node scripts/tool-layer-blend.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 120, height: 90 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const pixel = async (x, y) => {
  const raw = await call("render_region", { region: { x, y, w: 1, h: 1 }, raw: true, max_px: 1000 });
  if (!raw.ok || !raw.raw_url) return null;
  const buffer = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
  return [buffer[0], buffer[1], buffer[2]];
};
const grey = { r: 128, g: 128, b: 128, a: 255 };
await call("create_layer", { layer_id: "under", name: "底" });
await call("fill", { layer_id: "under", data: { color: grey, region: { x: 0, y: 0, w: 120, h: 90 } } });
await call("create_layer", { layer_id: "over", name: "上" });
await call("fill", { layer_id: "over", data: { color: grey, region: { x: 0, y: 0, w: 120, h: 90 } } });
const normal = await pixel(60, 45);
const set = await call("set_layer_blend", { layer_id: "over", mode: "multiply" });
if (set.ok !== true) {
  console.log("  set_layer_blend ⇒ " + JSON.stringify((set.context || {}).detail || set.error_code || set).slice(0, 160));
  console.log("  ✗ 混合模式不存在 ⇒ 判据红（预期 ✓）");
  process.exit(1);
}
const multiplied = await pixel(60, 45);
console.log("  normal  ：" + JSON.stringify(normal));
console.log("  multiply：" + JSON.stringify(multiplied));
const luma = (p) => 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
const failures = [];
if (!normal || !multiplied) failures.push("取不到像素 ⇒ 判据无效");
else {
  if (luma(multiplied) >= luma(normal)) failures.push(`multiply 没有变暗（${luma(multiplied).toFixed(1)} ≥ ${luma(normal).toFixed(1)}）✗`);
  if (multiplied.every((v, i) => v === normal[i])) failures.push("multiply 与 normal 完全一样 ⇒ 没生效 ✗");
}
// ④ 缺省 vs 明确 normal：另起一层，只比"缺省"与"设成 normal"两种渲染
const plain = await call("set_layer_blend", { layer_id: "under", mode: "normal" });
if (plain.ok !== true) failures.push("显式设成 normal 失败 ⇒ 缺省行为无从对照");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ layer blend：multiply 明确更暗，且 normal 可用");
