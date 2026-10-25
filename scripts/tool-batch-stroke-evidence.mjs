#!/usr/bin/env node
// **D1：`batch` 每笔取证摘要 `stroke_evidence`**（期 2b）：
//   摘要与 `calls` 同下标（`stroke_evidence.len() == count`）；每笔笔触给 `landed` /
//   `painted_pixels_clipped` / `ink_mass` / `max_channel_delta`（字段名与 `brush_stroke`
//   回执一致；`medium_stroke` 的 `paint_delta` 在此归一为 `max_channel_delta`）；
//   失败的笔带完整 `error`（与 `failed_calls` 里的同一份渲染错误）。
//
// **判据**（每臂独立文档，免得前一笔污染后一臂；`silent: true` 显式传 ——
//   本分支 `silent` 缺省仍是 `false`，期 2 的缺省改动不在本包）：
//   ① 10 笔 `brush_stroke` ⇒ 顶层 `ok:true`、`ok_count + fail_count == count`、
//      `stroke_evidence` 长 10、每笔四字段都是数值且与子结果回执的同名字段一致、
//      `0 ≤ ink_mass ≤ ink_footprint_pixels × 255`、响应无图（`preview` null、
//      子结果无 `image`、整份 JSON 里没有 `"image"` 键）；
//   ② 10 笔里 1 笔用不存在的笔刷 ⇒ 顶层 `ok:false`（A6）、`fail_count == 1`、
//      失败笔摘要 `ok:false` + `error.error_code == "reference_not_found"`、
//      该 `error` 与 `failed_calls[0].error` 是同一份，其余 9 笔仍有数值摘要；
//   ③ 1 笔 `medium_stroke` ⇒ 摘要四字段数值，`painted_pixels_clipped` 等于回执
//      `painted_pixels`、`max_channel_delta` 等于回执 `paint_delta`；
//   ④ 1 笔 `draw_shape`（非笔触）⇒ 摘要只有 `{index, tool, ok: true}`，不编取证数字。
//
// **变异**：删掉一次 `push` ⇒ ① 的长度恒等式红；把 `paint_delta` 的回落删掉 ⇒ ③ 红；
//   把失败摘要的 `error` 换成空对象 ⇒ ② 红；把 `failed` 判成「看 `value.ok`」⇒
//   ② 里失败笔的摘要仍可能红（子结果的 `ok:false` 在 `ok_response` 里被工具字段覆盖的情形）。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-batch-stroke-evidence.mjs <base-url>"); process.exit(2); }

const mkdoc = async (doc) => {
  let created;
  try {
    created = await (await fetch(`${base}/api/documents`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }),
    })).json();
  } catch (error) {
    console.error(`  ⊘ 不可判：连不上服务（判据前提不成立，不是产品问题）：${String(error).slice(0, 80)}`);
    process.exit(3);
  }
  if (!created?.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
  return created.token;
};
const call = (doc, token, tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=` + token, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

const failures = [];
const check = (ok, msg) => {
  if (!ok) failures.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};
const fail = (message) => { console.error(`  ✗ ${message}`); process.exit(1); };
const strokeCall = (i) => ({
  tool: "brush_stroke",
  arguments: {
    layer_id: "L", brush: "100%_Opaque",
    points: [[40, 30 + i * 18, 1.0], [280, 30 + i * 18, 1.0]], size: 30,
    color: [0.45, 0.38, 0.30],
  },
});
const fourNumeric = (entry) =>
  typeof entry?.landed === "boolean"
  && typeof entry?.painted_pixels_clipped === "number"
  && typeof entry?.ink_mass === "number"
  && typeof entry?.max_channel_delta === "number";

// ① 10 笔
const tenDoc = `d1-ten-${process.pid}`;
const tenToken = await mkdoc(tenDoc);
await call(tenDoc, tenToken, "create_layer", { layer_id: "L", name: "l" });
const ten = await call(tenDoc, tenToken, "batch", {
  calls: Array.from({ length: 10 }, (_, i) => strokeCall(i)), silent: true,
});
if (ten.ok !== true) fail(`① 10 笔 batch 失败 ⇒ ${JSON.stringify(ten).slice(0, 200)}`);
check(ten.count === 10, `① count=${ten.count}（应为 10）`);
check(ten.ok_count + ten.fail_count === ten.count, `① ok_count + fail_count == count（${ten.ok_count}+${ten.fail_count}==${ten.count}）`);
check(Array.isArray(ten.stroke_evidence) && ten.stroke_evidence.length === 10,
  `① stroke_evidence 长 ${ten.stroke_evidence?.length}（应为 10，与 calls 同下标）`);
check(ten.preview === null, `① 静默批处理顶层 preview=${JSON.stringify(ten.preview)}（应为 null）`);
check(!JSON.stringify(ten).includes('"image"'), `① 整份响应里不应有 "image" 键（无图）`);
let tenAllMatch = Array.isArray(ten.stroke_evidence) && ten.stroke_evidence.length === 10;
let tenAllNumeric = tenAllMatch;
for (let i = 0; i < 10 && tenAllMatch; i += 1) {
  const entry = ten.stroke_evidence[i];
  const result = ten.calls?.[i]?.result ?? {};
  if (!fourNumeric(entry)) { tenAllNumeric = false; break; }
  if (entry.index !== i || entry.tool !== "brush_stroke" || entry.ok !== true) { tenAllMatch = false; break; }
  if (entry.landed !== result.landed
    || entry.painted_pixels_clipped !== result.painted_pixels_clipped
    || entry.ink_mass !== result.ink_mass
    || entry.max_channel_delta !== result.max_channel_delta) { tenAllMatch = false; break; }
  if (!(entry.ink_mass <= (result.ink_footprint_pixels ?? 0) * 255)) { tenAllMatch = false; break; }
  if (result.image !== undefined || (result.preview !== undefined && result.preview !== null)) { tenAllMatch = false; break; }
}
check(tenAllNumeric, `① 每笔摘要四字段都应是数值（landed 布尔 + 三个数值）`);
check(tenAllMatch, `① 每笔摘要与子结果回执同名字段一致、index 同下标、墨量 ≤ 足迹×255、子结果无图`);

// ② 1 笔故意失败
const failDoc = `d1-fail-${process.pid}`;
const failToken = await mkdoc(failDoc);
await call(failDoc, failToken, "create_layer", { layer_id: "L", name: "l" });
const calls2 = Array.from({ length: 10 }, (_, i) => strokeCall(i));
calls2[4].arguments.brush = "不存在的笔刷.myb";
const bad = await call(failDoc, failToken, "batch", { calls: calls2, silent: true });
check(bad.ok === false, `② 有子调用失败 ⇒ 顶层 ok=${bad.ok}（应为 false，A6）`);
check(bad.ok_count === 9 && bad.fail_count === 1, `② ok_count=${bad.ok_count}、fail_count=${bad.fail_count}（应为 9/1）`);
check(Array.isArray(bad.stroke_evidence) && bad.stroke_evidence.length === 10,
  `② stroke_evidence 长 ${bad.stroke_evidence?.length}（应为 10，失败笔的摘要也不能缺）`);
const failedEntry = bad.stroke_evidence?.[4];
check(failedEntry?.ok === false, `② 失败笔摘要 ok=${failedEntry?.ok}（应为 false）`);
check(failedEntry?.error?.error_code === "reference_not_found",
  `② 失败笔 error_code=${failedEntry?.error?.error_code}（应为 reference_not_found）`);
check(typeof failedEntry?.error?.context?.detail === "string",
  `② 失败笔 error 应带可读 detail`);
check(JSON.stringify(failedEntry?.error) === JSON.stringify(bad.failed_calls?.[0]?.error),
  `② 失败笔摘要的 error 应与 failed_calls[0].error 是同一份`);
let restNumeric = Array.isArray(bad.stroke_evidence) && bad.stroke_evidence.length === 10;
for (let i = 0; i < 10 && restNumeric; i += 1) {
  if (i === 4) continue;
  if (!fourNumeric(bad.stroke_evidence[i])) restNumeric = false;
}
check(restNumeric, `② 其余 9 笔仍有数值摘要`);

// ③ medium_stroke 归一
const medDoc = `d1-medium-${process.pid}`;
const medToken = await mkdoc(medDoc);
await call(medDoc, medToken, "create_layer", { layer_id: "L", name: "l" });
const med = await call(medDoc, medToken, "batch", {
  calls: [{ tool: "medium_stroke", arguments: {
    layer_id: "L", medium: "oil",
    points: [[40, 120, 1.0], [280, 120, 1.0]], size: 30, color: [0.45, 0.38, 0.30],
  } }],
  silent: true,
});
if (med.ok !== true) fail(`③ medium batch 失败 ⇒ ${JSON.stringify(med).slice(0, 200)}`);
const medResult = med.calls?.[0]?.result ?? {};
const medEntry = med.stroke_evidence?.[0];
check(fourNumeric(medEntry), `③ medium 摘要四字段都应是数值（归一后）`);
check(medEntry?.painted_pixels_clipped === medResult.painted_pixels,
  `③ medium 摘要 clipped=${medEntry?.painted_pixels_clipped}（应等于回执 painted_pixels=${medResult.painted_pixels}）`);
check(medEntry?.max_channel_delta === medResult.paint_delta,
  `③ medium 摘要 delta=${medEntry?.max_channel_delta}（应等于回执 paint_delta=${medResult.paint_delta}）`);

// ④ 非笔触子调用
const shapeDoc = `d1-shape-${process.pid}`;
const shapeToken = await mkdoc(shapeDoc);
await call(shapeDoc, shapeToken, "create_layer", { layer_id: "L", name: "l" });
const shape = await call(shapeDoc, shapeToken, "batch", {
  calls: [strokeCall(0), { tool: "draw_shape", arguments: { layer_id: "L", object_id: "s1",
    data: { geometry: { kind: "rect", bbox: { x: 10, y: 10, w: 40, h: 30 } },
            color: { r: 10, g: 10, b: 10, a: 255 } } } }],
  silent: true,
});
if (shape.ok !== true) fail(`④ shape batch 失败 ⇒ ${JSON.stringify(shape).slice(0, 200)}`);
check(Array.isArray(shape.stroke_evidence) && shape.stroke_evidence.length === 2,
  `④ stroke_evidence 长 ${shape.stroke_evidence?.length}（应为 2）`);
check(fourNumeric(shape.stroke_evidence?.[0]), `④ 第 0 笔（笔触）摘要四字段数值`);
const s1 = shape.stroke_evidence?.[1];
check(s1?.index === 1 && s1?.tool === "draw_shape" && s1?.ok === true
  && s1?.landed === undefined && s1?.painted_pixels_clipped === undefined
  && s1?.ink_mass === undefined && s1?.max_channel_delta === undefined
  && s1?.error === undefined,
  `④ 第 1 笔（draw_shape）摘要应只有 {index, tool, ok}（不编取证数字），实测 ${JSON.stringify(s1)}`);

if (failures.length) {
  console.log(`\n  ✗ ${failures.length} 处不符（D1 batch 每笔取证摘要）`);
  process.exit(1);
}
console.log("\n  ✓ D1 batch 每笔取证摘要：10 笔全量 + 失败笔 + medium 归一 + 非笔触不编数全对");
