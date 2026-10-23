#!/usr/bin/env node
// **`smart_texture` 必须接受 `wait_for_render`（并透传给内层笔触）** ✓（第 639 轮；外部 `report.md` D6）
//
// **⇒ 症状**：外层工具只把 `preview:false` 透传给内层 `write_brush_stroke`，
//   **没透传 `wait_for_render`** ⇒ 客户端想关也关不了（框架级参数不让传）✗
//   ⇒ 实测 `smart_texture` **22.4 ms/笔** vs 直接 `brush_stroke` **6.6 ms/笔**（**3.4×**）。
//
// **⇒ 判据**：`smart_texture {wait_for_render: false}` 必须**被接受**（旧行为：`invalid_argument`
//   「不接受参数 wait_for_render」✗），且**真的落下笔触**（`ok_count>0`）✓。
//
// **变异**：把 spec 里那条 `wait_for_render` 参数删掉 ⇒ 回到 `invalid_argument` ⇒ 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-smart-texture-wait-passthrough.mjs <base> <doc> <token>");
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
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};

const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
// **画布尺寸从 `get_document` 读** ✓（第 639 轮实测：判据曾在 64×64 的 `_bootstrap` 上
//   用 200×200 的矩形 ⇒ 落笔全在画布外 ⇒ 39 笔全失败 ✗ ⇒ 那是**判据的错**，不是产品 ✓）
const summary = await call("get_document", {});
const docW = Number(summary.width ?? 64);
const docH = Number(summary.height ?? 64);
const box = {
  x: Math.round(docW * 0.1),
  y: Math.round(docH * 0.1),
  w: Math.max(8, Math.round(docW * 0.6)),
  h: Math.max(8, Math.round(docH * 0.6)),
};
console.log(`  画布 ${docW}×${docH}｜肌理区域 ${JSON.stringify(box)}`);
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: docW, h: docH },
  color: { r: 200, g: 180, b: 160, a: 255 },
  opacity: 1.0,
});
if (filled.ok !== true) fail(`fill_region ⇒ ${JSON.stringify(filled).slice(0, 140)}`);

const textured = await call("smart_texture", {
  layer_id: "layer_default",
  shape: { type: "rect", ...box },
  texture_type: "canvas",
  size: 6,
  wait_for_render: false,
});
const detail = String(textured.context?.detail ?? "");
if (textured.ok !== true) {
  if (detail.includes("wait_for_render")) {
    fail(`smart_texture 仍然**不接受** wait_for_render ⇒ 内层笔触的渲染等待关不掉（外部 D6）：${detail.slice(0, 120)}`);
  }
  console.error("  [probe] " + JSON.stringify(textured).slice(0, 1200));
  fail(`smart_texture 失败（可能是别的参数）`);
}
const strokes = Number(textured.strokes ?? 0);
const failed = Number(textured.failed ?? 0);
if (!(strokes > 0)) fail(`wait_for_render=false 时也必须真的落笔（strokes=${strokes}｜failed=${failed}）`);
if (failed !== 0) fail(`有笔触失败（failed=${failed}）⇒ 先看 first_error：${String(textured.first_error ?? "").slice(0, 160)}`);
console.log(`✓ smart_texture 接受 wait_for_render=false，并落下 ${strokes} 笔（failed=0）`);
