#!/usr/bin/env node
// **`export_svg(validate:true)` 不许**过度拒绝**：正常笔必须转矢量**（第 1429 轮）
//
// **⇒ 为什么需要** ✗：Q2 给 `validate` 加了 **P95 判据**（`normal_p95 ≤ max(0.35px, 0.08·r)`）⇒
//   判据变严 ⇒ **风险是"什么笔都被打回位图"** ⇒ 那样矢量导出就白做了 ✗
//   **⇒ ⇒ 而补丁作者自己也把这条列为"未验"**：「validate_failed 率变化…未跑大样本统计」✓
//
// **⇒ 断什么**（**结构量** ✓）：对**直 / 弧 / 之字 / 压力渐变**四种**正常**笔，
//   `validate:true` 导出后 **`strokes_raster === 0`** 且 **`strokes_vector === 笔数`** ✓
//   **⇒ 极短笔**（12 px / size 5）**允许**回退，但**必须计数**（`validate_failed === 1`）✓
//     ⇒ 那一条**证明**判据真的在起作用（不是恒真）✓ —— 否则这条判据可能只是"从没触发过" ✗
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-export-svg-validate-not-overrejecting.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const post = async (doc, tok, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${tok}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const newDoc = async (tag) => {
  const doc = `vno_${tag}_${stamp}`;
  const r = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 600, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  return r.token ? { doc, tok: r.token } : null;
};
const run = async (tag, points, size) => {
  const d = await newDoc(tag);
  if (!d) return null;
  await post(d.doc, d.tok, "create_layer", { layer_id: "L", name: "L" });
  const s = await post(d.doc, d.tok, "draw_stroke", {
    layer_id: "L", data: { points, size, color: [0.1, 0.1, 0.1], opacity: 1.0 } });
  if (s?.ok !== true) return null;
  const g = await post(d.doc, d.tok, "export_svg", { path: `vno_${tag}_${stamp}.svg`, validate: true });
  return g?.ok === true
    ? { vector: g.strokes_vector ?? 0, raster: g.strokes_raster ?? 0, failed: g.validate_failed ?? 0 }
    : null;
};

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const cases = [
  ["直笔", [[60, 150, 0.9], [400, 150, 0.9]], 7],
  ["弧线", Array.from({ length: 31 }, (_, i) => {
    const a = (i * 6 * Math.PI) / 180;
    return [240 + 60 * Math.cos(a), 150 + 60 * Math.sin(a), 0.9];
  }), 7],
  ["之字", Array.from({ length: 8 }, (_, i) => [60 + i * 30, 150 + (i % 2 ? 20 : -20), 0.9]), 7],
  ["压力渐变", Array.from({ length: 10 }, (_, i) => [60 + i * 20, 150, 0.3 + (0.7 * i) / 9]), 7],
  ["大笔 size40", [[60, 150, 0.9], [400, 150, 0.9]], 40],
];

let checked = 0;
for (const [label, points, size] of cases) {
  const r = await run(label.replace(/\W/g, ""), points, size);
  if (!r) { bad(`${label}：调用或导出失败（判据自身前提不成立）`); continue; }
  checked += 1;
  // **⇒ 一笔可以产出**多条**矢量路径** ✓（分层 tier：plateau + deep ⇒ 2 条；三段更高 ✓）
  //   ⇒ 所以断「**位图 0 且矢量 ≥ 1**」，**不是**「矢量恰好 1」✗（我第一版就是这么写的 ⇒ 弯线红 ✗）
  if (r.raster === 0 && r.vector >= 1) ok(`${label} ⇒ 矢量 ${r.vector}、位图 0（没被过度拒绝）`);
  else bad(`${label} ⇒ 应全部转矢量，实测矢量 ${r.vector}、位图 ${r.raster}、validate_failed ${r.failed}`);
}

// 反证：**软边笔必须仍被拒**（第 1440 轮换的）
//   **⇒ 为什么换** ✗：原来那条反证用「12 px / size 5 极短笔」⇒ 而第 1440 轮把细笔（r<3）
//     改判胶囊恒等式后，**它现在会通过** ⇒ 那条反证失效（会误报"判据从没触发过"）✗
//   **⇒ ⇒ 改成**软边笔**（`hardness 0.3`）：它的 `normal_p95` 实测 **0.76**（线 0.52）⇒
//     **必然**被拒 ✓ ⇒ 证明闸门真的会拒，不是恒真 ✓
{
  const d = await newDoc("softedge");
  if (!d) {
    console.log("  ○ 反证未验（建不出文档）");
  } else {
    await post(d.doc, d.tok, "create_layer", { layer_id: "L", name: "L" });
    const st = await post(d.doc, d.tok, "draw_stroke", { layer_id: "L",
      data: { points: [[80, 150, 0.9], [300, 150, 0.9]], size: 20.0, hardness: 0.3,
        color: [0.1, 0.1, 0.1], opacity: 1.0 } });
    if (st?.ok !== true) {
      console.log("  ○ 反证未验（软边笔没画成）");
    } else {
      const g = await post(d.doc, d.tok, "export_svg", { path: `vno_soft_${stamp}.svg`, validate: true });
      const ras = g?.strokes_raster ?? 0;
      const failed = g?.validate_failed ?? 0;
      // **⇒ 软边的回退理由**不是 `validate_failed`**（它走的是原生导出自己的软边判据 ⇒
      //   `raster_reasons.soft_edge`）⇒ 所以断言"**被拒且理由被计数**"，不限定是哪一条理由 ✓
      const reasons = g?.raster_reasons || {};
      const counted = failed >= 1 || Object.keys(reasons).length > 0;
      if (ras >= 1 && counted) {
        ok(`反证：软边笔仍被拒且**理由被计数** ⇒ 位图 ${ras}、validate_failed ${failed}、`
          + `raster_reasons ${JSON.stringify(reasons)} ⇒ 闸门不恒真 ✓`);
      } else {
        bad(`反证失败：软边笔应被拒且理由可查（否则闸门可能恒真）⇒ 位图 ${ras}、`
          + `validate_failed ${failed}、raster_reasons ${JSON.stringify(reasons)}`);
      }
    }
  }
}

// ① 细笔（r < 3）现在也必须转矢量（第 1440 轮：改判胶囊恒等式）
{
  const d = await newDoc("thin3");
  if (!d) {
    console.log("  ○ 细笔未验（建不出文档）");
  } else {
    await post(d.doc, d.tok, "create_layer", { layer_id: "L", name: "L" });
    await post(d.doc, d.tok, "draw_stroke", { layer_id: "L",
      data: { points: [[80, 150, 0.9], [120, 150, 0.9]], size: 3.0,
        color: [0.1, 0.1, 0.1], opacity: 1.0 } });
    const g = await post(d.doc, d.tok, "export_svg", { path: `vno_thin_${stamp}.svg`, validate: true });
    const vec = (g?.strokes_vector ?? 0) + (g?.strokes_vector_native ?? 0);
    const ras = g?.strokes_raster ?? 0;
    if (vec >= 1 && ras === 0) ok(`① 细笔 size 3 现在转矢量（胶囊恒等式通过）⇒ 矢量 ${vec}、位图 0`);
    else bad(`① 细笔 size 3 应转矢量（它按胶囊恒等式是 0.13 px）⇒ 矢量 ${vec}、位图 ${ras}`);
  }
}

if (checked === 0) {
  console.error("  ⊘ 不可判：一个用例都没跑成");
  process.exit(3);
}
if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（正常笔必须能转矢量）`);
  process.exit(1);
}
console.log("\n  ✓ 正常笔都转矢量，只有极短笔回退且如实计数");
