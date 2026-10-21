#!/usr/bin/env node
// **`export_svg` 的 `flatten_opacity` 必须与引擎**逐通道一致**（第 1411 轮）
//
// **⇒ 为什么** ✗：引擎在**线性光**空间合成，而 SVG 的 `fill-opacity` 在**sRGB 空间**混合
//   ⇒ 两者数学上不等 ⇒ 实测最坏差 **73 通道**（`check-svg-opacity-colour-space.mjs` 钉住）
//   ⇒ 所以导出器提供 `flatten_opacity: true`：**先在线性空间压平到背景**，emit **不透明**色 ✓
//
// **⇒ 断什么**（端到端，不用光栅器 ✓）：
//   ① 半透明矩形 + `flatten_opacity: true` ⇒ SVG 里的 `fill` **等于引擎采样值**（±1）
//   ② 同一图形 + `flatten_opacity: false` ⇒ SVG 里**必须**是 `fill-opacity` 形式（现状保留 ✓）
//   ③ 压平后**不许**再有 `fill-opacity`（否则又会被 sRGB 混合一次 ✗）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-export-svg-flatten-opacity.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const doc = `svgf_${stamp}`;
const made = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 64, height: 64,
    background: { r: 0, g: 0, b: 255, a: 255 } }),
})).json();
const TOK = made.token;
if (!TOK) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};

await post("create_layer", { layer_id: "L", name: "L" });
const filled = await post("fill_region", { layer_id: "L",
  shape: { type: "rect", x: 10, y: 10, w: 20, h: 20 }, color: [0, 0, 0], opacity: 0.5 });
if (filled?.ok !== true) { console.error("  ⊘ 不可判：落笔失败"); process.exit(3); }
if (filled.opacity !== 0.5) {
  console.log(`  ○ 注意：引擎回报 opacity=${filled.opacity}（期望 0.5）`);
}

const sample = await post("sample_colors", { points: [[20, 20]] });
const eng = (sample.samples || [])[0]?.rgba;
if (!eng) { console.error("  ⊘ 不可判：采不到引擎像素"); process.exit(3); }

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const exportAndRead = async (flag) => {
  const r = await post("export_svg", { path: `flat${Number(flag)}_${stamp}.svg`, flatten_opacity: flag });
  if (r?.ok !== true || !r.path) return null;
  return readFileSync(r.path, "utf8");
};

const withFlat = await exportAndRead(true);
const withoutFlat = await exportAndRead(false);
if (!withFlat || !withoutFlat) {
  console.error("  ⊘ 不可判：导出失败或读不到文件");
  process.exit(3);
}

// 取那个 rect 的标记
const pickRect = (svg) => {
  const m = svg.match(/<rect x="10"[^>]*>/);
  return m ? m[0] : null;
};
const parseFill = (markup) => {
  const m = markup && markup.match(/fill="#([0-9a-fA-F]{3,6})"/);
  if (!m) return null;
  let hex = m[1];
  if (hex.length === 3) hex = hex.split("").map((c) => c + c).join("");
  return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
};

// ① 压平后与引擎一致（±1）
const flatRect = pickRect(withFlat);
const flatColour = parseFill(flatRect);
if (!flatColour) {
  bad(`① 压平后的 rect 里找不到 fill ⇒ 实测 ${String(flatRect).slice(0, 90)}`);
} else {
  const d = Math.max(...[0, 1, 2].map((i) => Math.abs(flatColour[i] - eng[i])));
  if (d <= 1) ok(`① 压平色 ${JSON.stringify(flatColour)} == 引擎 ${JSON.stringify(eng.slice(0, 3))}（Δ ${d}）`);
  else bad(`① 压平色必须等于引擎采样值 ⇒ SVG ${JSON.stringify(flatColour)} vs 引擎 ${JSON.stringify(eng.slice(0, 3))}（Δ ${d}）`);
}

// ② 不压平时保留 fill-opacity（现状）
const rawRect = pickRect(withoutFlat);
if (rawRect && /fill-opacity/.test(rawRect)) ok(`② 不压平时保留 fill-opacity ⇒ ${rawRect.slice(0, 80)}`);
else bad(`② 不压平时应有 fill-opacity ⇒ 实测 ${String(rawRect).slice(0, 90)}`);

// ③ 压平后不许再有 fill-opacity
if (flatRect && !/fill-opacity/.test(flatRect)) ok("③ 压平后没有 fill-opacity（不会再被 sRGB 混合一次）");
else bad(`③ 压平后不该有 fill-opacity ⇒ 实测 ${String(flatRect).slice(0, 90)}`);

// ④ 半透明**描边**压不了 ⇒ 必须**出声**（第 1434 轮；补丁自己列的「已知界限：只压 fill」）
//   **⇒ 为什么压不了** ✗：描边骑在形状边缘上 ⇒ 一半压在 fill、一半压在背景
//     ⇒ **两个 backdrop** ⇒ 压成单色**必错一半** ✗（而 Q123 对 fill 的规矩正是「异质 ⇒ 不压」✓）
//   **⇒ ⇒ 所以**判据断的不是"压平了"，而是"**如实说了**" ✓
{
  // 本判据的 `post` 已绑定到单一文档 ⇒ 用**新图层**隔离（不污染 ①–③ 的采样 ✓）
  {
    await post("create_layer", { layer_id: "SL", name: "SL" });
    await post("draw_shape", { layer_id: "SL", data: {
      geometry: { kind: "rect", bbox: { x: 40, y: 40, w: 80, h: 80 } },
      color: [0, 0, 0, 0.5], stroke_color: [1, 1, 1, 0.5], stroke_width: 8 } });
    const r = await post("export_svg", { path: `flat_stroke_${stamp}.svg`, flatten_opacity: true });
    const w = r?.warnings;
    const said = Array.isArray(w) && w.some((x) => /半透明描边/.test(String(x)));
    if (r?.flatten_stroke_skipped >= 1 && said) {
      ok(`④ 半透明描边未压平但**出声**（flatten_stroke_skipped=${r.flatten_stroke_skipped}）`);
    } else {
      bad(`④ 半透明描边压不了 ⇒ 必须计数并警告 ⇒ 实测 skipped=${r?.flatten_stroke_skipped}、warnings=${JSON.stringify(w)}`);
    }
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（flatten_opacity 必须与引擎逐通道一致）`);
  process.exit(1);
}
console.log("\n  ✓ flatten_opacity 与引擎逐通道一致，且不压平时保留原形式");
