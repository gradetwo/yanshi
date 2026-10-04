// **参考图色差判据**（测试报告 §三.2）：`analyze_region` 加 `compare_with_reference` 后，
// AI 才从"只知道画面是什么"变成"知道离参考图差多少"。
// 判据必须**两侧都能判**（否则"永远返回 0"也能过 ✗）：
//   ① 没设参考图 ⇒ **明确作答**（ok:false + 原因 ✓），**不是**静默 0 ✗；
//   ② 把当前画面设为参考图 ⇒ 同一区域自比 ⇒ ΔE ≈ 0 ✓；
//   ③ 之后**再画几笔**改变画面 ⇒ ΔE 明显 > 阈值 ✓。
// 能红：把 compare_with_reference 的分支整段去掉（返回 null）⇒ ②③ 立刻红 ✓；
//       把"没有参考图"改成返回 0 ⇒ ① 立刻红 ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-reference-delta-e.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "refde_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 160, height: 120 }) })).json()).token;
const region = { x: 0, y: 0, w: 160, h: 120 };
const failures = [];

// ① 没设参考图
const before = await call(doc, token, "analyze_region", { region, compare_with_reference: true });
const cmp0 = before && before.comparison_with_reference;
console.log("  没参考图 ⇒ " + JSON.stringify(cmp0).slice(0, 150));
if (!cmp0 || cmp0.ok !== false) failures.push("没有参考图时没有明确作答 ⇒ 可能是静默给了一个值");

// 画几笔并把它设为参考图
for (let i = 0; i < 4; i++) {
  await call(doc, token, "brush_stroke", { layer_id: "layer_default", brush: "100%_Opaque",
    points: [[20 + i * 30, 30, 0.9], [40 + i * 30, 90, 0.9]], size: 14,
    color: { r: 200, g: 80, b: 40, a: 255 } });
}
const rendered = await call(doc, token, "render_region", { region, include_image: true, max_px: 1_000_000 });
const blobUrl = JSON.stringify(rendered || "");
const hashMatch = blobUrl.match(/([0-9a-f]{64})/);
console.log("  render_region 的 blob=" + (hashMatch ? hashMatch[1].slice(0, 12) + "…" : "（没找到 64 位哈希）"));
if (!hashMatch) failures.push("从 render_region 拿不到 blob 哈希 ⇒ 判据无法继续（先看它的返回形状）");
else {
  const set = await call(doc, token, "set_reference", { blob_hash: hashMatch[1] });
  console.log("  set_reference ⇒ " + JSON.stringify(set).slice(0, 120));
  if (set && set.ok === false) failures.push("set_reference 失败：" + JSON.stringify(set).slice(0, 140));

  // ② 自比 ⇒ ΔE ≈ 0
  const same = await call(doc, token, "analyze_region", { region, compare_with_reference: true });
  const cmp1 = same && same.comparison_with_reference;
  console.log("  自比 ⇒ " + JSON.stringify(cmp1).slice(0, 170));
  if (!cmp1 || cmp1.ok !== true) failures.push("设了参考图后仍没有 ok:true 的比较结果");
  else if (!(cmp1.delta_e_mean < 1.0)) failures.push(`同一画面自比的 ΔE 均值应 ≈ 0，实为 ${cmp1.delta_e_mean}`);

  // ③ 改变画面 ⇒ ΔE 明显变大
  await call(doc, token, "fill_region", { layer_id: "layer_default",
    shape: { type: "rect", x: 0, y: 0, w: 160, h: 120 },
    color: { r: 20, g: 140, b: 200, a: 255 } });
  const changed = await call(doc, token, "analyze_region", { region, compare_with_reference: true });
  const cmp2 = changed && changed.comparison_with_reference;
  console.log("  改画面后 ⇒ " + JSON.stringify(cmp2).slice(0, 170));
  if (!cmp2 || cmp2.ok !== true) failures.push("改变画面后没有拿到比较结果");
  else if (!(cmp2.delta_e_mean > 5.0)) failures.push(`改变画面后 ΔE 均值应明显 > 5，实为 ${cmp2.delta_e_mean}`);
}

if (failures.length) { console.log("  ✗ 参考图色差不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ 参考图色差：缺参考图时明确作答、自比≈0、改画面后明显增大");
process.exit(0);
