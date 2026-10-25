#!/usr/bin/env node
// **`painted_pixels_clipped` 是主口径，`painted_pixels` 只是裁剪前的面积**（A3）
//
// **⇒ 问题**：`painted_pixels` 是裁剪前的铺开面积（印章面积），用它算覆盖率会高估；
//   位图选区裁剪时可能一个像素都不落而回执照样 `ok:true`。
//   并存方案：保留 `painted_pixels`，新增同值字段 `painted_pixels_raw`；
//   registry 的 summary 写明"覆盖率/落墨量以 clipped 为准"。
//
// **判据**：
//   ① 选区裁掉一半的笔 ⇒ `painted_pixels_clipped` 约为 `painted_pixels` 的一半
//      （相对关系断言，不写死像素数；对称笔迹，左右两半等长）；
//   ② `painted_pixels_raw` 与 `painted_pixels` 同值。
//
// **变异**：`painted_pixels_clipped` 改成 raw 值 ⇒ ① 红（比值 ≈ 1，不进一半区间）；
//   `painted_pixels_raw` 改成 clipped 值 ⇒ ② 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-clipped-primary.mjs <base> <doc> <token>");
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
const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};

const layer = await call("create_layer", { layer_id: "layer_clipped", name: "clipped" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
// 左半选区（320×240 文档的左半 0..160），一笔横贯整幅 ⇒ 左右等长，对称裁一半
const sel = await call("create_selection", {
  selection_id: "s-half",
  shape: { kind: "rect", x: 0, y: 0, w: 160, h: 240 },
});
if (sel.ok !== true) {
  console.log("  ⊘ 不可判：建不了左半选区（判据前提不成立）");
  process.exit(3);
}
const r = await call("brush_stroke", {
  layer_id: "layer_clipped",
  brush: "100%_Opaque.myb",
  points: [
    [20, 120],
    [300, 120],
  ],
  size: 30,
  color: [1, 0, 0, 1],
  clip_to_selection: "s-half",
});
if (r.ok !== true) fail(`brush_stroke（半裁）⇒ ${JSON.stringify(r).slice(0, 160)}`);
if (!(r.painted_pixels > 0)) {
  console.log("  ⊘ 不可判：这笔 raw 面积为 0 ⇒ 一半关系无从谈起");
  process.exit(3);
}
const ratio = r.painted_pixels_clipped / r.painted_pixels;
console.log(`  raw=${r.painted_pixels}、clipped=${r.painted_pixels_clipped}、比值=${ratio.toFixed(3)}`);
check(
  ratio >= 0.35 && ratio <= 0.65,
  `① 半裁之笔 clipped 约为 raw 的一半：比值 ${ratio.toFixed(3)} ∈ [0.35, 0.65]`,
);
check(
  r.painted_pixels_raw === r.painted_pixels,
  `② painted_pixels_raw 与 painted_pixels 同值：${r.painted_pixels_raw} === ${r.painted_pixels}`,
);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（clipped 为主口径）`);
  process.exit(1);
}
console.log("\n  ✓ clipped 为主口径：半裁比值约一半 + raw 别名同值");
