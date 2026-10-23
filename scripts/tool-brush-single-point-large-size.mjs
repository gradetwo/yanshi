#!/usr/bin/env node
// **大笔刷的"单击"必须真的落下一枚 dab** ✓（第 640 轮；外部 `report.md` 的 D1）
//
// **⇒ 缺陷**：短笔触兜底位移写死 `4.0`，而引擎的 **dab 间距 ≈ size/4** ⇒
//   `size=176` 时间距 44 ≫ 位移 4 ⇒ **一枚 dab 都不盖** ✗，而回执仍是 `ok:true`（静默无操作 ✗）。
//   同一个语义在三处各写一个数（`paint_mask` 0.25×size、`brush_deposit_mask` 单点 4.0、多点阈值 4.0）✗。
//
// **⇒ 判据**：单点笔触 + **大 size**（这里用 64 ⇒ 间距 ≈16 ≫ 旧位移 4）⇒ `painted_pixels > 0` ✓。
//
// **变异**：把 `deposit_nudge` 改回 `4.0`（或让 `brush_deposit_mask` 用 4.0）⇒ 判据红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-single-point-large-size.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  console.error(`  ✗ create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
  process.exit(1);
}
const summary = await call("get_document", {});
const docW = Number(summary.width ?? 64);
const docH = Number(summary.height ?? 64);
const size = Math.min(64, Math.floor(Math.min(docW, docH) * 0.75));
const cx = Math.round(docW / 2);
const cy = Math.round(docH / 2);
const painted = await call("brush_stroke", {
  layer_id: "layer_default",
  brush: "calligraphy",
  points: [[cx, cy, 0.5]],
  size,
  color: [0.9, 0.2, 0.2, 1.0],
});
console.log(
  `  画布 ${docW}×${docH}｜单点 (${cx},${cy})｜size=${size}（dab 间距 ≈ ${(size / 4).toFixed(1)}）｜painted=${painted.painted_pixels}｜ok=${painted.ok}`,
);
if (painted.ok !== true) {
  console.error(`  ✗ brush_stroke 本身失败：${JSON.stringify(painted).slice(0, 160)}`);
  process.exit(1);
}
const pixels = Number(painted.painted_pixels ?? 0);
if (!(pixels > 0)) {
  console.error(
    `  ✗ 大笔刷单点一笔都没落（painted_pixels=${pixels}）⇒ 兜底位移小于 dab 间距（外部 D1：写死 4.0）`,
  );
  process.exit(1);
}
console.log(`✓ 大笔刷单点落笔 ${pixels} px（兜底位移随 size ⇒ 间距被跨过）`);
