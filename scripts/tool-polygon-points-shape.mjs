#!/usr/bin/env node
// **多边形点形状判据**（第 3 轮 ✓，收口"静默成功" ✗）
//
// **为什么需要它** ✗（用户现场实测 ＋ 我复现 ✓）：
//   `fill_region` 传**字典点** `[{x,y},…]` 时，服务端**照收** ✓：
//     `ok:true`、`dirty_bbox:null` ✗、**192 个瓦片被弄脏** ✗（数组点对照只有 **6** 个 ✓）
//   ⇒ **几何没被解析，API 却报成功** ⇒ 用户画面里"没有这个形状" ✗
//   ⇒ **∴ 这正是本仓头号病根「说成功了其实没按语义画」** ✓✓
//   （该现象在 `crates/yanshi-server/src/tools.rs` 的 polygon 分支**不做形状校验**时成立 ✓。）
//
// **判据（两条，各自能红 ✓）**：
//   ① **字典点必须被拒** ✓（`ok === false`，且错误体**指出正确形状** `[x, y]` ✓）
//      —— 变异检验：把 polygon 分支的校验去掉 ⇒ 它又变回 `ok:true` ⇒ **本判据变红** ✓。
//   ② **数组点必须照常画出** ✓（`ok === true` 且 `dirty_bbox` **非 null** ✓）
//      —— 这条**防作弊** ✗：否则"一律拒绝多边形"也能让 ① 通过，而那样产品就废了 ✓。
//
// **不写死观测** ✓：只断言 `ok` 的**真假**与 `dirty_bbox` 的**有无** ✓（语义边界 ✓）；
//   不比较瓦片数、不比较耗时 ✓（那些随文档与实现变 ✓）。
//
// 用法：node scripts/tool-polygon-points-shape.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-polygon-points-shape.mjs <base-url>");
  process.exit(2);
}

const failures = [];
function check(ok, label, detail) {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
}

const doc = "poly-shape-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
})).json();
if (!created.token) {
  console.error("✗ 前置不成立：建不出文档 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const token = created.token;
const call = async (tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();

const layer = await call("create_layer", { layer_id: "L1" });
if (layer.ok !== true) {
  console.error("✗ 前置不成立：建不出图层 ⇒ " + JSON.stringify(layer).slice(0, 160));
  process.exit(2);
}

// —— ① 字典点：必须被拒，且错误体给出正确形状 ——
const dict = await call("fill_region", {
  layer_id: "L1",
  shape: { type: "polygon", points: [{ x: 100, y: 100 }, { x: 400, y: 100 }, { x: 250, y: 400 }] },
  color: { r: 255, g: 0, b: 0, a: 255 },
});
const detail = String((dict.context || {}).detail || "");
console.log(`  字典点 ⇒ ok=${dict.ok}｜error=${dict.error_code}｜detail=${detail.slice(0, 120)}`);
check(dict.ok === false, "字典点必须被**响亮拒绝**（ok === false）", "ok=" + dict.ok);
check(
  /\[x, y\]|\[x,y\]/.test(detail),
  "拒绝时必须指出**正确形状**（错误体里出现 `[x, y]`）",
  detail.slice(0, 90),
);

// —— ② 数组点：必须照常画出（防"一律拒绝"）——
const array = await call("fill_region", {
  layer_id: "L1",
  shape: { type: "polygon", points: [[100, 100], [400, 100], [250, 400]] },
  color: { r: 0, g: 255, b: 0, a: 255 },
});
const drawn = array.drawn || {};
console.log(
  `  数组点 ⇒ ok=${array.ok}｜dirty_bbox=${JSON.stringify(drawn.dirty_bbox)}｜dirty_tiles=${drawn.dirty_tiles}`,
);
check(array.ok === true, "数组点必须照常被接受（ok === true）", "ok=" + array.ok);
check(
  drawn.dirty_bbox !== null && drawn.dirty_bbox !== undefined,
  "数组点必须真的算出**包围盒**（dirty_bbox 非 null）",
  "dirty_bbox=" + JSON.stringify(drawn.dirty_bbox),
);

console.log("");
if (failures.length) {
  console.error(`结论：多边形点形状不合格 ✗（${failures.length} 条）`);
  for (const item of failures) console.error("   - " + item);
  process.exit(1);
}
console.log("结论：✓ 字典点被响亮拒绝、数组点照常画出（语义边界判据，与瓦片数/耗时无关）");
process.exit(0);
