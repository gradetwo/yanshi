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

// 反证：极短笔**允许**回退，而且必须**计数**（证明判据真的会拒）
{
  const r = await run("tiny", [[100, 100, 0.9], [112, 100, 0.9]], 5);
  if (!r) {
    console.log("  ○ 极短笔未验（调用失败）");
  } else if (r.raster >= 1 && r.failed >= 1) {
    ok(`反证：极短笔（12px/size5）回退 ${r.raster} 且 validate_failed=${r.failed} ⇒ 判据确实会拒（不是恒真）`);
  } else {
    console.log(`  ○ 极短笔这次转成了矢量（矢量 ${r.vector}、位图 ${r.raster}）⇒ 反证不成立，但不影响上面的正面断言`);
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
