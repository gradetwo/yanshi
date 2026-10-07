#!/usr/bin/env node
// **单点笔触必须被响亮拒绝**判据（第 13 轮 ✓）
//
// **为什么要守它** ✗：一个**单点**不成笔触 ✓（工程上至少要两点才谈得上"笔迹" ✓）。
// 若这条校验被放松 ✗，表现就是"**调用报成功、画面上什么都没有**" ✗ —— 正是本仓**头号病根**
//（"说能用其实不能用" ✓）。第 339 轮实测：单点在多档尺寸下都返回
// `ok=false` ＋ `precondition_failed` ✓（**响亮** ✓），而**两点**（相距 40px）正常画出
// `painted_pixels=366` ✓ ⇒ 行为**正确** ✓ —— 本判据就是把这个"正确"钉住 ✓。
//
// **判据（两条，各自能红 ✓）**：
//   ① **单点必须被响亮拒绝** ✓：`ok === false` **且** `error_code` 非空 ✓
//      —— 只看 `ok === false` 不够 ✗：错误体必须**说得出原因** ✓（否则就是"失败了但不知道为什么" ✓）。
//      **变异**：把"点数 < 2 就拒"那条校验去掉 ⇒ `ok` 变 `true` ⇒ **判据红** ✓。
//   ② **两点必须照常画出** ✓（**防作弊** ✗）：否则"一律拒绝笔触"也能让 ① 通过 ✓，而产品就废了 ✓。
//
// **不写死观测** ✓：只看 `ok` 的真假与 `error_code` 的有无 ✓（语义边界 ✓），不看像素数、不看耗时 ✓。
//
// 用法：node scripts/tool-single-point-rejected.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-single-point-rejected.mjs <base-url>");
  process.exit(2);
}

const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const doc = "single-point-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 1024, height: 768 }),
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

const stroke = (id, points, size) => ({
  layer_id: "L1", object_id: id, brush: "100%_Opaque", size, color: { r: 0, g: 0, b: 0, a: 255 },
  preview: false, points,
});

// —— ① 单点必须被响亮拒绝（多档尺寸都试 ✓ —— 免得不小心只守住一档）——
for (const size of [1, 8, 20]) {
  const r = await call("brush_stroke", stroke("one" + size, [[300, 300, 1]], size));
  const code = r.error_code || r.error || null;
  console.log(`  size ${String(size).padStart(2)} 单点 ⇒ ok=${r.ok}｜error_code=${code}`);
  check(r.ok === false, `size ${size} 的**单点**必须被拒绝（ok === false）`, "ok=" + r.ok);
  check(code !== null && String(code).length > 0,
    `size ${size} 的**单点**被拒时必须给出**非空 error_code**（说得出原因 ✓）`,
    "error_code=" + code);
}

// —— ② 两点必须照常画出（防"一律拒绝"✗）——
const two = await call("brush_stroke", stroke("two", [[400, 400, 1], [450, 400, 1]], 12));
console.log(`  两点（相距 50px） ⇒ ok=${two.ok}｜painted=${two.painted_pixels}｜tiles=${two.dirty_tiles}`);
check(two.ok === true, "**两点**笔触必须照常被接受（ok === true）", "ok=" + two.ok);
check(Number(two.painted_pixels || 0) > 0, "**两点**笔触必须真的画出像素（painted_pixels > 0）",
  "painted_pixels=" + two.painted_pixels);

console.log("");
if (failures.length) {
  console.error(`结论：单点校验不合格 ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 单点被响亮拒绝、两点照常画出（语义边界判据，与像素数/耗时无关）");
process.exit(0);
