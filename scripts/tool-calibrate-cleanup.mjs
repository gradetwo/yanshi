#!/usr/bin/env node
// **★ 校准的"自清理"不许吃掉调用前已有的内容 ✗ ★**（2026-10-16 ✓）
//
// **∴ 来源 ✗**：第三方 skill 的 CAL-CLEANUP 报告 ✓ ＋ **我本机独立复现** ✓：
//   "创建 N 个标记和一次建层后，校准调用 `undo_last(count=N+1)` ✓；
//    **undo_last 按绘制笔迹计数，建层不算一笔** ⇒ **可能额外撤掉调用前的内容** ✗"
//   实测（修前）：
//     调用前 L1 有 ["keep_me"] ⇒ `calibrate{cleanup:true}`（5 个标记 ⇒ count=6）⇒ 撤 6 笔
//     ⇒ 事后 L1 = **[]** ✗（keep_me 被吃掉 ✓），而回执还报 `undone=0`／`passed=true` ✗ ★
//
// **∴ 判什么（两半都要 ✓ —— 只判"没吃掉"会被"什么都不做"骗过 ✗）**：
//   ① **调用前的内容一个都不能少** ✓：对象（`keep_me` ✓）、图层数、锚点 ✓ 全部不变 ✓；
//   ② **校准自己的东西必须清干净** ✓：`__calibration*` 图层在调用后**不在** ✓
//      （既不吃用户的 ✓、也不留自己的 ✓）。
//
// **∴ 能红 ✓**：把清理改回 `undo_last{count: 标记数+1}` ⇒ ① 立刻红 ✓（本机修前实测就是这个形态 ✓）。
//
// 用法：node scripts/tool-calibrate-cleanup.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-calibrate-cleanup.mjs <base-url>");
  process.exit(2);
}
const DOC = `calclean_${Date.now().toString(36)}`;
const call = async (token, tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${DOC}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const objectIds = async (token) => {
  const reply = await call(token, "list_objects", { layer_id: "L1" });
  const list = (reply && (reply.objects || reply.items)) || [];
  return Array.isArray(list) ? list.map((o) => o.id || o.object_id).sort() : null;
};
const layerIds = async (token) => {
  const reply = await call(token, "list_layers", {});
  const list = (reply && (reply.layers || reply.items)) || [];
  return Array.isArray(list) ? list.map((l) => l.id || l.layer_id).sort() : null;
};
const anchorNames = async (token) => {
  const reply = await call(token, "get_anchors", {});
  const raw = (reply && (reply.anchors || reply.items)) || {};
  return Object.keys(raw).sort();
};

const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: 320, height: 240 }),
})).json();
if (!created || !created.token) {
  console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
  process.exit(3);
}
const token = created.token;
await call(token, "create_layer", { layer_id: "L1", name: "L1" });
const stroke = await call(token, "brush_stroke", {
  layer_id: "L1", object_id: "keep_me", brush: "100%_Opaque", size: 12,
  color: { r: 200, g: 60, b: 30, a: 255 }, points: [[40, 40], [160, 120]],
});
if (!stroke || stroke.ok !== true) {
  console.error(`❌ 判不了：先在 L1 画一笔失败（${JSON.stringify(stroke).slice(0, 120)}）⇒ 退出码 3`);
  process.exit(3);
}
await call(token, "set_anchor", { name: "keep_anchor", x: 100, y: 100 });
const beforeObjects = await objectIds(token);
const beforeLayers = await layerIds(token);
const beforeAnchors = await anchorNames(token);
console.log(`  调用前：对象 ${JSON.stringify(beforeObjects)}｜图层 ${JSON.stringify(beforeLayers)}｜锚点 ${JSON.stringify(beforeAnchors)}`);

const calibrated = await call(token, "calibrate", { cleanup: true });
console.log(`  calibrate{cleanup:true} ⇒ ${JSON.stringify(calibrated && { ok: calibrated.ok, passed: calibrated.passed, cleanup: calibrated.cleanup, layer_id: calibrated.layer_id }).slice(0, 160)}`);

const afterObjects = await objectIds(token);
const afterLayers = await layerIds(token);
const afterAnchors = await anchorNames(token);
console.log(`  调用后：对象 ${JSON.stringify(afterObjects)}｜图层 ${JSON.stringify(afterLayers)}｜锚点 ${JSON.stringify(afterAnchors)}`);

const failures = [];
if (!calibrated || calibrated.ok !== true) failures.push(`calibrate 本身没成功 ✗（${JSON.stringify(calibrated).slice(0, 140)}）`);
// ① 调用前的内容一个都不能少 ✓
if (JSON.stringify(beforeObjects) !== JSON.stringify(afterObjects)) {
  failures.push(`**调用前已有的对象被弄没了** ✗：${JSON.stringify(beforeObjects)} ⇒ ${JSON.stringify(afterObjects)}（**这正是那条 undo_last{count} 的病 ✓**）`);
}
if (!Array.isArray(afterLayers) || afterLayers.includes("L1") === false) {
  failures.push(`L1 不见了 ✗（图层：${JSON.stringify(afterLayers)} ✓）`);
}
if (JSON.stringify(beforeAnchors) !== JSON.stringify(afterAnchors)) {
  failures.push(`锚点变了 ✗：${JSON.stringify(beforeAnchors)} ⇒ ${JSON.stringify(afterAnchors)}`);
}
// ② 校准自己的东西要清干净 ✓
const strays = (afterLayers || []).filter((id) => String(id).startsWith("__calibration"));
if (strays.length) failures.push(`校准自己的图层没清掉 ✗：${JSON.stringify(strays)}（既不吃用户的、也不留自己的 ✓）`);

if (failures.length) {
  console.error(`❌ ${failures.length} 项没成立：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(`✓ 校准自清理两半都对：调用前的内容一个没少（对象/图层/锚点 ✓）＋ 自己的 __calibration 图层清干净 ✓`);
