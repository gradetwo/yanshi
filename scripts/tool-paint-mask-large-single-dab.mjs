#!/usr/bin/env node
// **`paint_mask` 的单点 + 大笔刷必须真的沉积**（否则 `deposited:false` 静默无操作）✓
// （第 641 轮；外部 `report.md` 的 D1 —— `brush_deposit_mask` 的兜底位移写死 `4.0` ✗）
//
// **⇒ 为什么不用 `brush_stroke` 做这条判据** ✗：实测把兜底位移改回 `4.0`，
//   `brush_stroke` 的 `painted_pixels` **仍然是 1883** ⇒ 说明它**不走** `brush_deposit_mask`
//   ⇒ 那条判据**覆盖不到**这段代码 ✓（第 640 轮的教训，写在这里免得再犯 ✗）。
//
// **⇒ 判据**：`create_mask` 建一个蒙版 ⇒ `paint_mask` 单点 + `size=48`（dab 间距 ≈12）
//   ⇒ `deposited` 必须是 `true` ✓（旧代码位移 4 < 12 ⇒ `false` ✗）。
//
// **变异**：把 `deposit_nudge` 改回 `4.0` ⇒ `deposited:false` ⇒ 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-paint-mask-large-single-dab.mjs <base> <doc> <token>");
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
const size = Math.min(48, Math.floor(Math.min(docW, docH) * 0.7));
const cx = Math.round(docW / 2);
const cy = Math.round(docH / 2);
const maskId = `mask_dab_${Date.now().toString(36)}`;
// `create_mask` 的参数是 `mask_id/shape/...`（**没有 `layer_id`** ✓ 蒙版挂在当前图层 ✓）
const mask = await call("create_mask", {
  mask_id: maskId,
  shape: { kind: "rect", bbox: { x: 0, y: 0, w: docW, h: docH } },
});
if (mask.ok !== true) {
  console.error(`  ✗ create_mask ⇒ ${JSON.stringify(mask).slice(0, 160)}`);
  process.exit(mask.error_code === "invalid_argument" ? 1 : 3);
}
const painted = await call("paint_mask", {
  mask_id: maskId,
  points: [[cx, cy, 0.5]],
  size,
  // **`paint_mask` 没有 `brush` 参数** ✓（它有 `preset`；缺省笔刷够用 ✓）
});
console.log(
  `  画布 ${docW}×${docH}｜单点 (${cx},${cy})｜size=${size}（dab 间距 ≈ ${(size / 4).toFixed(1)}）｜deposited=${painted.deposited}｜ok=${painted.ok}`,
);
if (painted.ok !== true) {
  console.error(`  ✗ paint_mask 本身失败：${JSON.stringify(painted).slice(0, 160)}`);
  process.exit(1);
}
if (painted.deposited !== true) {
  console.error(
    `  ✗ 单点 + 大笔刷**零沉积**（deposited=${painted.deposited}）⇒ 兜底位移小于 dab 间距（外部 D1：写死 4.0）`,
  );
  process.exit(1);
}
console.log("✓ paint_mask：单点 + 大笔刷真的沉积了（兜底位移随 size ⇒ 跨过 dab 间距）");
