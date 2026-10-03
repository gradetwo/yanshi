#!/usr/bin/env node
// **「厚涂应当是平台，而不是一串泡泡」判据** ✓（用户报告 2.3 "膨胀香蕉/蚕蛹" ✗）。
// 根因（已定位 ✓）：`yanshi-medium-oil` 里每个 dab 都按**自己的边缘**与**自己的颗粒相位**取值
// ⇒ 沿轴线形成**周期性暗环**（86 枚 dab / 340px ⇒ 周期 ≈ 4px）✗。
//
// **量法与其自证**（上一版栽过两次 ✓）：
// ① **不平滑** ✗ —— 我试过 5px 滑动平均，窗口比 4px 的周期还大 ⇒ 把被测结构抹平 ⇒ 修前修后都是 0 ✗；
// ② **不看绝对自相关** ✗ —— 常数信号的绝对自相关天然≈1（实测平坦块 0.989 ✗）⇒
//    改看**形状差值** `acf(4) − acf(2)` ✓：4px 周期会让 acf(4) 高于 half-period 的 acf(2) ✓；
//    平坦/平滑信号两者都 ≈1 ⇒ 差值 ≈0 ✓（实测平坦块 0.000 ✓）。
// ③ **正对照自证** ✓：同一量法先用 `fill` 画出的**平坦块**测一遍 ✓ ⇒ 那里**必须无周期** ✓，
//    否则说明"这个量法对什么都报警" ✗ ⇒ 判据无效 ✓（**判据先自证，再判产品** ✓）。
// ④ **层序** ✓：对照层在**下**、笔触层在**上** ✓（上一版把对照层建在后面 ⇒ 盖住笔触 ⇒ 整幅同色 ✗）。
const base = process.argv[2];
const docId = process.argv[3] || "oil6";
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
const sampleRow = async (y, x0, x1) => {
  const raw = await call("render_region", { region: { x: 0, y: 0, width: 400, height: 160 }, raw: true, max_px: 200000 });
  if (!raw.ok || !raw.raw_url) return null;
  const buffer = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
  const width = raw.width || 400;
  const line = [];
  for (let x = x0; x <= x1; x += 1) {
    const i = (y * width + x) * 4;
    line.push(0.299 * buffer[i] + 0.587 * buffer[i + 1] + 0.114 * buffer[i + 2]);
  }
  return { line, width, buffer };
};
const periodicContrast = (line) => {
  const mean = line.reduce((a, b) => a + b, 0) / line.length;
  const centered = line.map((v) => v - mean);
  const variance = centered.reduce((a, v) => a + v * v, 0);
  if (variance <= 0) return 0;
  const acf = (lag) => {
    let sum = 0, count = 0;
    for (let i = 0; i + lag < centered.length; i += 1) { sum += centered[i] * centered[i + lag]; count += 1; }
    return sum / (variance * (count / centered.length));
  };
  return acf(4) - acf(2);
};
const countDips = (line) => {
  let count = 0;
  for (let i = 6; i < line.length - 6; i += 1) {
    let lt = 0, rt = 0;
    for (let k = 1; k <= 6; k += 1) { if (line[i - k] > line[i] + 3) lt += 1; if (line[i + k] > line[i] + 3) rt += 1; }
    if (lt >= 5 && rt >= 5) count += 1;
  }
  return count;
};
const failures = [];
// ---------- ① 对照层（在**下** ✓）+ 平坦块 ----------
const flatLayer = await call("create_layer", { layer_id: "flat_layer", name: "对照" });
const flatFill = await call("fill", { layer_id: "flat_layer", data: { color: { r: 170, g: 95, b: 60, a: 255 }, region: { x: 0, y: 0, w: 400, h: 160 } } });
if (!flatLayer.ok || !flatFill.ok) { console.log("  ✗ 平坦对照建不起来 ⇒ 无法自证 ⇒ 判据无效"); process.exit(1); }
// ---------- ② 笔触层（在**上** ✓）+ 一条密集 oil 笔触 ----------
const strokeLayer = await call("create_layer", { layer_id: "stroke_layer", name: "厚涂" });
if (!strokeLayer.ok) { console.log("  ✗ 笔触层建不起来 ⇒ 判据无效"); process.exit(1); }
const points = [];
for (let x = 30; x <= 370; x += 4) points.push([x, 80, 0.6]);
const stroke = await call("medium_stroke", { layer_id: "stroke_layer", medium: "oil", points, size: 30, color: { r: 170, g: 95, b: 60, a: 255 } });
if (stroke.ok !== true) { console.log("  ✗ 画不出油笔触 ⇒ 判据无效 ⇒ " + JSON.stringify((stroke.context || {}).detail || stroke.error_code)); process.exit(1); }
// ---------- ③ 采样：对照行（无笔触处 y=120 ✓）与笔触中心线（y=80 ✓）----------
const flatRow = await sampleRow(120, 60, 340);
const strokeRow = await sampleRow(80, 40, 360);
const bgRow = await sampleRow(10, 200, 200);
if (!flatRow || !strokeRow || !bgRow) { console.log("  ✗ 采样失败 ⇒ 判据无效"); process.exit(1); }
const flatContrast = periodicContrast(flatRow.line);
const strokeContrast = periodicContrast(strokeRow.line);
const dips = countDips(strokeRow.line);
const ink = strokeRow.line[160], background = bgRow.line[0];
console.log(`  正对照（fill 平坦块）acf(4)-acf(2) = ${flatContrast.toFixed(3)}   ← **必须 ≈0**，否则量法瞎报`);
console.log(`  被测笔触 dabs=${stroke.dabs}｜acf(4)-acf(2) = ${strokeContrast.toFixed(3)}｜暗环数=${dips}（占 dab ${(dips / stroke.dabs).toFixed(2)}）`);
console.log(`  笔触亮度=${ink.toFixed(0)}｜背景亮度=${background.toFixed(0)}`);
// ---------- ④ 自证：量法对平坦块必须不报警 ----------
if (Math.abs(flatContrast) > 0.15) { console.log(`  ✗ 连平坦块都报周期（${flatContrast.toFixed(3)}）⇒ 量法不可用 ⇒ 判据无效 ✗`); process.exit(1); }
// ---------- ⑤ 判产品 ----------
if (Math.abs(ink - background) < 15) failures.push("笔触与背景几乎没差别 ⇒ 判据无效");
if (strokeContrast > 0.05) failures.push(`中心线呈 4px 周期（acf(4)-acf(2)=${strokeContrast.toFixed(2)}）⇒ 一串泡泡 ✗`);
if (dips / Math.max(1, stroke.dabs) > 0.10) failures.push(`暗环占 dab 数的 ${(dips / stroke.dabs).toFixed(2)} ⇒ 每枚 dab 自带边缘 ✗`);
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 厚涂中心线是平台（无 4px 周期、暗环占比低），且笔触清晰可见");
