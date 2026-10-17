#!/usr/bin/env node
// **`speed_gain` 必须真的生效** ✓（第 1000+ 轮 ✓；Muse 的 P2-10 ✓）
//
// **∴ 背景 ✓**：`oil-06-paint.myb` 的 `dabs_per_second.base_value = 0.0` ✗
//   ⇒ **∴ `speed1_gamma`／`speed2_gamma`／`speed1_slowness`／`speed2_slowness` 四档
//      **只在笔触速度变化时才有差异**✗ ⇒ **∴ `dabs = 0` ⇒ 它们恒不起作用 ⇒ 设了等于没设 ✗**
//   ⇒ **∴ 修法 ✓**：设 `speed_gain` 时若 `dabs_per_second <= 0` ⇒ 补 **60** ✓
//
// **∴ 这条判据怎么验 ✓**：**变速笔触** ✗（**∴ 匀速的话四档恒无差异 ✗ ⇒ **∴ 测不出 ✓**）
//   前半**密集**（慢）＋ 后半**稀疏**（快）✓ ⇒ 两条：不带 `brush_params` ／ 带 `{"speed_gain": 1.0}`
//   ⇒ **∴ 两张导图**必须不同**✓**（**∴ 若 `dabs` 仍为 0 ⇒ 应当**完全相同**✗ ⇒ **∴ 判据红 ✓**）
//   **∴ 实测（修后）✓**：`plain.png` **5857 字节** vs `gain.png` **5234 字节** ⇒ **差 623 字节** ✓
import { readFileSync, existsSync } from "node:fs";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-speed-gain.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
// 前半密集（慢）→ 后半稀疏（快）
const PTS = [[40, 100], [50, 100], [60, 100], [70, 100], [80, 100], [90, 100], [100, 100], [140, 100], [200, 100], [280, 100], [360, 100]];

const run = async (layer, params) => {
  await call("create_layer", { layer_id: layer, name: layer });
  const stroke = await call("brush_stroke", {
    layer_id: layer, brush: "oil-06-paint",
    color: { r: 90, g: 60, b: 40, a: 255 },
    points: PTS.map(([x, y]) => [x, y, 1]),
    brush_params: params, preview: false,
  });
  if (stroke.ok !== true) { console.log(`  ✗ ${layer}: brush_stroke 失败 ${JSON.stringify(stroke).slice(0, 140)}`); process.exit(1); }
  const png = await call("export_png", { layer_id: layer, path: `sg_${layer}.png` });
  const path = png?.path;
  if (!path) { console.log("  ✗ export_png 没给 path"); process.exit(3); }
  if (!existsSync(path)) { console.log(`  ✗ 导出的文件不存在：${path}（判据跑不了）`); process.exit(3); }
  return readFileSync(path);
};

const plain = await run("sg_plain", {});
const gain = await run("sg_gain", { speed_gain: 1.0 });
let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };
check(plain.length > 0 && gain.length > 0, `两张导图都非空（plain ${plain.length} B／gain ${gain.length} B）`);
check(!plain.equals(gain), `变速笔触下，带 speed_gain 与不带**必须不同**（plain ${plain.length} B vs gain ${gain.length} B）`);
if (bad) { console.log(`  ✗ speed_gain 有 ${bad} 处不满足（-${Math.abs(plain.length - gain.length)} B）`); process.exit(1); }
console.log(`  ✓ speed_gain 生效：变速笔触下两张导图不同（${plain.length} B vs ${gain.length} B，差 ${Math.abs(plain.length - gain.length)} B）`);
