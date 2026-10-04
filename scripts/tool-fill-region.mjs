#!/usr/bin/env node
// **`fill_region` 判据** ✓（需求文档 P0-1.1 ✓：一次完成带**形状**的区域填充 ✓）。
// 现状（先查过 ✓）：`fill`（矩形/区域）与 `draw_shape`（形状）**都已存在** ✓ ⇒ 多半是**薄包装** ✓。
// 判据做**几何检查** ✓（不是"变了没有"这种弱断言 ✗）：
//   ① `shape:{type:"ellipse",cx,cy,rx,ry}` 被接受且落下像素 ✓；
//   ② **椭圆的中心**必须被填上 ✓（取该点像素与所填颜色一致 ✓）；
//   ③ **包围盒的四个角**必须**没被填** ✗（矩形包围盒的四角在椭圆之外 ✓）
//      —— 这一条专防"把椭圆当矩形填了"✗（那种实现①②都过 ✓，只有③能抓住 ✓）。
const base = process.argv[2];
const docId = process.argv[3] || "fr1";
if (!base) { console.error("用法: node scripts/tool-fill-region.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 300, height: 200 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const rgba = async (x, y) => {
  const raw = await call("render_region", { region: { x, y, width: 1, height: 1 }, raw: true, max_px: 1000 });
  if (!raw.ok || !raw.raw_url) return null;
  const buffer = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
  return [buffer[0], buffer[1], buffer[2], buffer[3]];
};
await call("create_layer", { layer_id: "layer_default", name: "l" });
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "ellipse", cx: 150, cy: 100, rx: 100, ry: 60 },
  color: { r: 220, g: 40, b: 40, a: 255 },
  opacity: 1.0,
});
if (filled.ok !== true) {
  console.log("  fill_region ⇒ " + JSON.stringify((filled.context || {}).detail || filled.error_code || filled).slice(0, 160));
  console.log("  ✗ 工具没成功 ⇒ 判据红（预期 ✓）");
  process.exit(1);
}
const centre = await rgba(150, 100);
const corners = { 左上: await rgba(51, 41), 右上: await rgba(248, 41), 左下: await rgba(51, 158), 右下: await rgba(248, 158) };
console.log("  椭圆中心像素：" + JSON.stringify(centre));
console.log("  包围盒四角：" + JSON.stringify(corners));
const near = (pixel, r, g, b) => pixel && Math.abs(pixel[0] - r) <= 24 && Math.abs(pixel[1] - g) <= 24 && Math.abs(pixel[2] - b) <= 24;
const failures = [];
if (!near(centre, 220, 40, 40)) failures.push(`椭圆中心没被填成指定色 ⇒ ${JSON.stringify(centre)}`);
for (const [name, pixel] of Object.entries(corners)) {
  if (near(pixel, 220, 40, 40)) failures.push(`包围盒${name}角也被填了 ⇒ 椭圆被当矩形 ✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ fill_region(ellipse)：中心填上、四角留白 ⇒ 形状是真的按几何填的");
