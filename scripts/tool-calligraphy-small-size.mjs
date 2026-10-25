#!/usr/bin/env node
// **P2-13：`calligraphy` 小 size 必须落墨**（AA 钳制）。
//
// **根因**：`vendor/hokusai-core/src/stroke.rs` 的抗锯齿分支在
// `current_fadeout < aa_min` 时算 `hardness_new`，小半径下为负 ⇒
// `draw_dab_default` 在 `hardness <= 0` 时整 dab 拒画 ⇒
// `calligraphy` 在 size 1/2/3 全不落墨、size 4 起正常（阈值是 size<4，不是 size<8）。
//
// **判据**：size 1–8 × 短路径（约 20px，3 点，跨 dab 间距）/长路径（约 200px，3 点）
// 矩阵，每格都必须 `ok:true` 且 `painted_pixels > 0`。每格用**独立图层**
// （同一图层上重叠落墨会让后一笔的 painted_pixels 缩水）。
//
// **变异**：撤掉守卫（`hardness_new > 0.0 &&` 删掉）⇒ size 1–3 的格红。
// **前提失败必须 exit(3)**：拿不到 token/建不了层 ⇒ 不可判，不是产品失败。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-calligraphy-small-size.mjs <base-url>"); process.exit(2); }
const doc = "csz_" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 1600, height: 800 }),
})).json();
const token = created.token;
if (!token) { console.error("  ⊘ 不可判：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};

const failures = [];
// 短路径约 20px、长路径约 200px（3 点；都远跨 dab 间距 ⇒ 0 墨只能是 AA 的锅，
// 不是“路径太短”——那是另一条分支，与 AA 无关）。
const paths = {
  short: (x, y) => [[x, y, 0.85], [x + 10, y + 5, 0.85], [x + 20, y, 0.85]],
  long: (x, y) => [[x, y, 0.85], [x + 100, y + 5, 0.85], [x + 200, y, 0.85]],
};
for (let size = 1; size <= 8; size++) {
  for (const [name, pts] of Object.entries(paths)) {
    const layer = `L${size}_${name}`;
    const made_layer = await call("create_layer", { layer_id: layer });
    if (made_layer.ok !== true) {
      console.error(`  ⊘ 不可判：建层失败 ${layer} ⇒ ${JSON.stringify(made_layer).slice(0, 140)}`);
      process.exit(3);
    }
    const x = 100 + (size - 1) * 180;
    const y = name === "short" ? 150 : 450;
    const made = await call("brush_stroke", {
      layer_id: layer, brush: "calligraphy", size,
      color: [0.9, 0.2, 0.2, 1.0], points: pts(x, y),
    });
    const painted = Number(made.painted_pixels ?? 0);
    console.log(`  size=${size} × ${name} ⇒ ok=${made.ok}｜painted=${painted}`);
    if (made.ok !== true || !(painted > 0)) {
      failures.push(`size=${size} × ${name} 应当落墨（ok=${made.ok}，painted=${painted}）：${JSON.stringify(made).slice(0, 160)}`);
    }
  }
}
if (failures.length) {
  console.log("  ✗ calligraphy 小 size 落墨不合格：");
  for (const f of failures) console.log(`     - ${f}`);
  process.exit(1);
}
console.log("  ✓ calligraphy size 1–8 × 短/长路径全部落墨（P2-13 AA 钳制已修）");
process.exit(0);
