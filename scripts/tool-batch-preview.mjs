#!/usr/bin/env node
// **`batch` 的 `preview: true` 判据** ✓（需求文档 P0-2「实时预览流」✓；画师报告："一轮 30 秒"✗）。
// 现状（写判据前先读规格 ✓）：`batch` 的参数里**没有** `preview` ✗ ⇒ 判据现在必然红 ✓。
// 判据（不猜形状 ✓，按 `render_region` 的既有约定来 ✓）：`batch` 带 `preview: true` 时，
// 返回顶层应当**带一张缩略图**（`image` 且含 `data` ✓，或 `thumb_url`/`preview` 之一 ✓）。
const base = process.argv[2];
const docId = process.argv[3] || "bp1";
if (!base) { console.error("用法: node scripts/tool-batch-preview.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 200, height: 150 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
await call("create_layer", { layer_id: "layer_default", name: "l" });
const batch = await call("batch", {
  preview: true,
  calls: [
    { tool: "brush_stroke", arguments: { layer_id: "layer_default", brush: "classic-brush", points: [[40, 75, 0.5], [160, 75, 0.5]], size: 18, color: { r: 40, g: 80, b: 160, a: 255 } } },
  ],
});
const keys = Object.keys(batch || {});
console.log("  batch(preview:true) ⇒ ok=" + batch.ok + "｜顶层键=" + keys.join(","));
const failures = [];
if (batch.ok !== true) failures.push("batch 没成功 ⇒ " + JSON.stringify((batch.context || {}).detail || batch.error_code || "?"));
else {
  const image = batch.image || batch.preview || null;
  const hasData = !!(image && (image.data || image.thumb_url || image.url));
  if (!hasData) {
    failures.push("batch 带了 preview:true，却没返回任何图（顶层键：" + keys.join(",") + "）⇒ 画师仍要手动再调 render_region（一轮 30 秒 ✗）");
  } else {
    console.log("  ✓ 拿到预览：" + JSON.stringify(image).slice(0, 120));
  }
  if (batch.preview === undefined && batch.image === undefined) {
    console.log("  （提示：可用字段名应当是 image / preview 之一 ✓；实际见上 ✓）");
  }
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ batch 直接带回预览图");
