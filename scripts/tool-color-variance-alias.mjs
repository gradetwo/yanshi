#!/usr/bin/env node
// **`color_variance` 别名不得把色相推飞** ✓（第 1000+ 轮 ✓；Muse 的 P0-1 ✓）
//
// **∴ 背景 ✓**：四组语义别名都走同一条展开（`tools.rs` 的 `apply_brush_params`）——
//   `for name in group { brush.get_mut(setting).base_value = number; }` ⇒ **同一个值、零缩放** ✗。
//   `color_variance` 组里的 `change_color_h` 是**色相** ✓ ⇒ **∴ `0.15` 就是满幅色相抖动 ✗**。
// **∴ 实测（修前）**：棕色 rgb(120,80,40) ＋ `color_variance: 0.15` ⇒ `sample_color` 返回
//   **rgba [117,165,46]** ✗ ⇒ 色相 **30.0° → 84.2°** ⇒ **漂 54.2°**（**∴ 棕变绿 ✓**）。
// **∴ 修后**（`alias_scale` 给这一组 0.2）⇒ **rgba [128,101,41]** ⇒ **41.4°** ⇒ **漂 11.4°** ✓。
//
// **用法**：node scripts/tool-color-variance-alias.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-color-variance-alias.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
const hue = ([r, g, b]) => {
  const [rr, gg, bb] = [r / 255, g / 255, b / 255];
  const max = Math.max(rr, gg, bb), min = Math.min(rr, gg, bb), d = max - min;
  if (d === 0) return 0;
  let h;
  if (max === rr) h = ((gg - bb) / d) % 6;
  else if (max === gg) h = (bb - rr) / d + 2;
  else h = (rr - gg) / d + 4;
  return ((h * 60) + 360) % 360;
};
const START = [120, 80, 40]; // 棕 ✓
const THRESHOLD = 15;        // 度 ✓（修前 54.2°、修后 11.4°）

await call("create_layer", { layer_id: "cv", name: "cv" });
const painted = await call("brush_stroke", {
  layer_id: "cv", brush: "100%_Opaque",
  color: { r: START[0], g: START[1], b: START[2], a: 255 },
  points: [[40, 40, 1], [360, 160, 1]],
  brush_params: { color_variance: 0.15 },
  preview: false,
});
if (painted.ok !== true) { console.log(`  ✗ brush_stroke 失败：${JSON.stringify(painted).slice(0, 160)}`); process.exit(1); }

const at = async (x, y) => {
  const r = await call("sample_color", { x, y });
  if (!Array.isArray(r.rgba)) { console.log(`  ✗ sample_color(${x},${y}) 没有 rgba ⇒ 判据跑不了`); process.exit(3); }
  return r.rgba.slice(0, 3);
};
const a = await at(80, 55);
const b = await at(340, 150);
const drift = Math.min(Math.abs(hue(a) - hue(b)), 360 - Math.abs(hue(a) - hue(b)));
const fromStart = Math.min(Math.abs(hue(a) - hue(START)), 360 - Math.abs(hue(a) - hue(START)));

let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };
check(drift < THRESHOLD, `同一笔两端色相一致（漂移 ${drift.toFixed(1)}° < ${THRESHOLD}°，两点 ${JSON.stringify(a)}／${JSON.stringify(b)}）`);
check(fromStart < THRESHOLD + 10, `相对起始色 rgb(${START}) 的色相漂移 ${fromStart.toFixed(1)}° < ${THRESHOLD + 10}°`);
if (bad) { console.log(`  ✗ color_variance 别名有 ${bad} 处不满足`); process.exit(1); }
console.log(`  ✓ color_variance: 0.15 只带来 ${fromStart.toFixed(1)}° 的色相变化（修前是 54.2°）`);
