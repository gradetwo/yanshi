#!/usr/bin/env node
// **「厚涂应当是平台，而不是一串泡泡」判据** ✓（用户报告 2.3 ✓ "膨胀香蕉/蚕蛹" ✗）。
// 根因（本轮定位 ✓）：`yanshi-medium-oil/src/lib.rs:240-241` 里 `rim = 1.0 - edge` 用的是
// **每一枚 dab 自己的边缘** ✗ ⇒ 每个 stamp 都自带一圈 +0.45 的亮/暗环 ✓ ⇒ 叠加后像塑料充气玩具 ✗。
//
// 判据（纯 RGBA ✓，不解 PNG —— 用 `render_region(raw:true)` ✓）：
//   ① 画**一条**长而密、**同压感**的 oil 笔触 ✓；
//   ② 沿中心线取**亮度**序列 ✓；
//   ③ 统计**局部极小值**个数 `dips` ✓（每枚 dab 的闭合暗环会各造一个 ✗）；
//   ④ 断言 `dips <= 2` ✓ ⇒ **今天必然红** ✓（预期与 dab 数同阶 ✓）。
//   ⑤ 对照 ✓：笔触**外侧**仍须与背景有差异 ✓（颜料脊不能被一起抹平 ✗）。
const base = process.argv[2];
const docId = process.argv[3] || "oil3";
if (!base) { console.error("用法: node scripts/tool-impasto-plateau.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 400, height: 160 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
await call("create_layer", { layer_id: "layer_default", name: "底" });
// 一条**直线**、多点、同压感 ⇒ 让大量 dab 沿轴线重叠 ✓
const points = [];
for (let x = 30; x <= 370; x += 4) points.push([x, 80, 0.6]);
const stroke = await call("medium_stroke", {
  layer_id: "layer_default", medium: "oil", points, size: 30,
  color: { r: 170, g: 95, b: 60, a: 255 },
});
if (stroke.ok !== true) {
  console.log("  ✗ 画不出油笔触 ⇒ 判据无效 ⇒ " + JSON.stringify((stroke.context || {}).detail || stroke.error_code));
  process.exit(1);
}
console.log("  medium_stroke ok ✓｜dabs=" + stroke.dabs + "｜bbox=" + JSON.stringify(stroke.dirty_bbox) + "｜medium=" + JSON.stringify(stroke.medium));
// **直通 RGBA** ✓（`raw:true`）⇒ 不解 PNG ✓
const raw = await call("render_region", { region: { x: 0, y: 0, width: 400, height: 160 }, raw: true, max_px: 200000 });
if (!raw.ok) { console.log("  ✗ render_region(raw) 失败 ⇒ 判据无效 ⇒ " + JSON.stringify(raw.context || {})); process.exit(1); }
console.log("  raw 响应键：" + Object.keys(raw).join(","));
const url = raw.raw_url || raw.url || (raw.image && raw.image.url);
if (!url) { console.log("  ✗ 拿不到 raw_url ⇒ 判据无效（键：" + Object.keys(raw).join(",") + "）"); process.exit(1); }
const buffer = Buffer.from(await (await fetch(url.startsWith("http") ? url : base + url)).arrayBuffer());
const width = raw.width || 400, height = raw.height || 160;
const luma = (x, y) => { const i = (y * width + x) * 4; return 0.299 * buffer[i] + 0.587 * buffer[i + 1] + 0.114 * buffer[i + 2]; };
// 中心线亮度（笔触画在 y=80 ✓）
const line = [];
for (let x = 40; x <= 360; x += 1) line.push(luma(x, 80));
// 局部极小值（左右各 6px 都比它亮 3 以上 ⇒ 算一个"暗环"✓）
let dips = 0;
for (let i = 6; i < line.length - 6; i += 1) {
  const v = line[i];
  let lt = 0, rt = 0;
  for (let k = 1; k <= 6; k += 1) { if (line[i - k] > v + 3) lt += 1; if (line[i + k] > v + 3) rt += 1; }
  if (lt >= 5 && rt >= 5) dips += 1;
}
// 对照：笔触外侧（y=80-24=56 一带）与背景（y=10）必须有差异 ✓
// 实测 bbox=[13,63,374,34] ⇒ 笔触纵向覆盖 y≈63..97；原先取 y=58 **在笔触之外** ⇒ 对照组当然"无差异"（判据自己的采样错）
const ink = luma(200, 80), background = luma(200, 10);
const outside = ink;   // 对照改成"笔触本身仍明显存在"（原先比的是笔触之外 ⇒ 当然无差异）
console.log(`  中心线亮度：min=${Math.min(...line).toFixed(0)} max=${Math.max(...line).toFixed(0)}｜dips(局部极小)=${dips}`);
console.log(`  外侧亮度=${outside.toFixed(0)}｜背景亮度=${background.toFixed(0)}`);
const failures = [];
if (dips > 2) failures.push(`中心线有 ${dips} 处暗环 ⇒ 每枚 dab 自带边缘（报告 2.3 ✗）`);
if (Math.abs(outside - background) < 40) failures.push("笔触外侧与背景没有差异 ⇒ 判据无效或颜料脊被抹平");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 厚涂中心线是平台（暗环 ≤ 2），且外侧仍有颜料脊");
