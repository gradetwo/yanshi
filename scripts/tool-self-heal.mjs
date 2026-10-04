// **自愈判据**（测试报告 §一.3）：报告说"重新连接会污染历史，且 MCP 没有 reset/clear/delete_layer，
// 只能去 shell 里 rm -rf"。**前半句很可能成立 ✓，后半句需要验证** ✓ ——
// `delete_layer` 其实是存在的 ✓。这条判据就用**现有工具**走一遍"清理 + 重新开始"：
// 若成功 ⇒ 报告关于"无法在 MCP 内自愈"的结论需要修正 ✓；若失败 ⇒ 坐实缺口 ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-self-heal.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "heal_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }) })).json()).token;

// ① 造"污染"：画几笔 + 加两层
const strokes = [];
for (let i = 0; i < 3; i++) {
  // **用 brush_stroke**：`draw_stroke` 的参数是 layer_id/data/object_id（不带 color）⇒ 之前那次根本没画上 ✗。
  strokes.push(await call(doc, token, "brush_stroke", { layer_id: "layer_default",
    brush: "100%_Opaque", points: [[20 + i * 30, 40, 0.8], [60 + i * 30, 90, 0.8]], size: 12,
    color: { r: 200, g: 60, b: 40, a: 255 } }));
}
console.log("  落笔：" + JSON.stringify(strokes.map((x) => x && x.ok)));
const add1 = await call(doc, token, "create_layer", { name: "polluted A" });
const add2 = await call(doc, token, "create_layer", { name: "polluted B" });
const before = await call(doc, token, "list_layers", {});
const objsBefore = await call(doc, token, "list_objects", {});
console.log(`  污染后：图层 ${(before.layers || []).length} 个、对象 ${(objsBefore.objects || []).length} 个` +
  `（create_layer: ${add1.ok}/${add2.ok}）`);

const failures = [];
// ② 只用现有工具自愈：删掉所有对象，再删掉除一个以外的所有图层
for (const o of (objsBefore.objects || [])) {
  const id = o.object_id || o.id;
  if (!id) continue;
  const del = await call(doc, token, "delete_object", { object_id: id });
  if (del && del.ok === false) failures.push(`delete_object(${id}) 失败：${JSON.stringify(del).slice(0, 120)}`);
}
const layers = (before.layers || []).map((l) => l.layer_id).filter(Boolean);
for (const id of layers.slice(1)) {
  const del = await call(doc, token, "delete_layer", { layer_id: id });
  if (del && del.ok === false) failures.push(`delete_layer(${id}) 失败：${JSON.stringify(del).slice(0, 120)}`);
}
const objsAfter = await call(doc, token, "list_objects", {});
const layersAfter = await call(doc, token, "list_layers", {});
console.log(`  清理后：图层 ${(layersAfter.layers || []).length} 个、对象 ${(objsAfter.objects || []).length} 个`);
if ((objsAfter.objects || []).length !== 0) failures.push(`清理后仍有 ${(objsAfter.objects || []).length} 个对象`);
if ((layersAfter.layers || []).length > 1) failures.push(`清理后仍有 ${(layersAfter.layers || []).length} 个图层（保留 1 个即可）`);

// ③ 清理后必须能接着画（这是"自愈"的真正含义）
const left = (layersAfter.layers || [])[0] || {};
const again = await call(doc, token, "brush_stroke", { layer_id: left.layer_id || "layer_default",
  brush: "100%_Opaque", points: [[40, 120, 0.9], [120, 160, 0.9]], size: 14,
  color: { r: 40, g: 120, b: 200, a: 255 } });
console.log("  清理后再画一笔 ⇒ " + JSON.stringify(again).slice(0, 140));
if (!again || again.ok === false) failures.push("清理后画不了 ⇒ 自愈路径不成立");

if (failures.length) { console.log("  ✗ 仅用现有工具无法自愈："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ 仅用现有工具即可自愈（无需 shell）：删对象 + 删多余图层 ⇒ 清空且可继续画");
process.exit(0);
