#!/usr/bin/env node
// **落笔回执必须顶层给结论**：`landed` + `landed_reason`（A1/B1）
//
// **⇒ 问题**：`brush_stroke` / `medium_stroke` 回执只给原始数
//   （`painted_pixels` / `painted_pixels_clipped` / `ink_footprint_pixels` /
//   `max_channel_delta` / `warnings`），不给结论 ⇒ 调用方每批要自己读五个字段
//   再调 `sample_colors` 才能确认"画上了"。
//
// **判据**：
//   ① 白底画白墨 ⇒ `landed:false` + reason `invisible_on_background`
//      （足迹 > 0 且 `max_channel_delta == 0` 的那支；白上画白）；
//   ② 位图选区裁空 ⇒ `landed:false` + reason `clipped_empty`
//      （`painted_pixels_clipped == 0` 的那支；矩形选区被减到只剩 1px 边成位图，笔落在选区外）；
//   ③ 正常一笔 ⇒ `landed:true` + reason `painted`；
//   ④ `medium_stroke` 正常一笔 ⇒ `landed:true` + 带 `ink_mass` / `ink_footprint_pixels`；
//   ⑤ `medium_stroke`（`pixel` 品类）白底画白墨 ⇒ `landed:false` + `clipped_empty`
//      （`pixel` 品类不改色 ⇒ `painted_pixels=0` ⇒ medium 的 clipped（=painted）为 0；
//      medium 无选区裁剪，此臂即"什么都没落下"的兜底；用**独立文档**跑（①–④ 的墨在主文档上，
//      共用会污染底色 ⇒ 916 轮教训）。
//      注意：⑤ **不是** `invisible_on_background` —— 介质笔的颜料要过介质模拟，
//      "白"落到画布上不是白（实测：`oil` 白 ⇒ 画布最大 RGB 差 53、`watercolor` 中心 214、
//      `marker` 中心 158 ⇒ 都是**真可见**，`landed:true` 才对）；只有 `pixel` 写色精确，
//      白 ⇒ 画布纹丝不动（`painted=0`）。所以 medium 的 `invisible_on_background`
//      在公开 API 上**无可达构造**（该臂由 `landed_reason` 的 Rust 单元测试覆盖，
//      画笔侧的 ① 臂端到端覆盖同一分支）。
//
// **变异**：把判定条件恒取反 ⇒ ①②③④⑤ 全红；把 medium 的 `landed` 恒写 `true` ⇒ ⑤ 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-landed.mjs <base> <doc> <token>");
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

// **① 之前先确认前提**：这张文档的底真是白的，否则"白上画白"造不出来
// （判据的前提不成立 ⇒ exit 3，不是产品失败）。
const blank = await call("sample_colors", { points: [[5, 5]] });
const blankRgb = (blank.samples ?? [])[0]?.rgba?.slice(0, 3).join(",");
if (blankRgb !== "255,255,255") {
  console.log(`  ⊘ 不可判：文档底不是白的（(5,5)=${blankRgb}）⇒ 白上画白的前提不成立`);
  process.exit(3);
}

const layer = await call("create_layer", { layer_id: "layer_landed", name: "landed" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
const strokePoints = [
  [40, 60, 0.8],
  [280, 180, 0.8],
];

// ① 白底画白墨
const white = await call("brush_stroke", {
  layer_id: "layer_landed",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [1, 1, 1, 1],
});
if (white.ok !== true) fail(`① brush_stroke（白墨）⇒ ${JSON.stringify(white).slice(0, 160)}`);
if (!(white.ink_footprint_pixels > 0)) {
  console.log("  ⊘ 不可判：白墨没落足迹（ink_footprint_pixels=0）⇒ 测的不是 invisible_on_background 那支");
  process.exit(3);
}
if (!(white.painted_pixels_clipped > 0)) {
  console.log("  ⊘ 不可判：白墨被裁空了（painted_pixels_clipped=0）⇒ 测的不是 invisible_on_background 那支");
  process.exit(3);
}
check(
  white.landed === false && white.landed_reason === "invisible_on_background",
  `① 白上画白 ⇒ landed=${white.landed} reason=${white.landed_reason}（足迹 ${white.ink_footprint_pixels}、delta ${white.max_channel_delta}）`,
);

// ② 位图选区裁空：矩形选区被减到只剩一条 1px 边（减掉自身会报"空选区"被拒 ⇒
//   减 49/50 高，留 y=59..60 一线），再画一笔与那条线完全不交 ⇒ 一笔都落不下。
//   选区是 bitmap 种（list_selections 验 kind），裁空是真裁空（painted_pixels_clipped=0）。
const selA = await call("create_selection", {
  selection_id: "s-empty",
  shape: { kind: "rect", x: 10, y: 10, w: 50, h: 50 },
});
const selB = await call("create_selection", {
  selection_id: "s-empty",
  mode: "subtract",
  shape: { kind: "rect", x: 10, y: 10, w: 50, h: 49 },
});
if (selA.ok !== true || selB.ok !== true) {
  console.log("  ⊘ 不可判：建不了减空的位图选区（判据前提不成立）");
  process.exit(3);
}
const listed = await call("list_selections", {});
const emptySel = (listed.selections ?? []).find((x) => x.selection_id === "s-empty");
if (!emptySel || emptySel.shape?.kind !== "bitmap") {
  console.log(`  ⊘ 不可判：减空之后不是 bitmap（kind=${emptySel?.shape?.kind}）⇒ 裁空前提不成立`);
  process.exit(3);
}
const clipped = await call("brush_stroke", {
  layer_id: "layer_landed",
  brush: "100%_Opaque.myb",
  points: [
    [200, 200],
    [280, 200],
  ],
  size: 30,
  color: [1, 0, 0, 1],
  clip_to_selection: "s-empty",
});
if (clipped.ok !== true) fail(`② brush_stroke（裁空）⇒ ${JSON.stringify(clipped).slice(0, 160)}`);
check(
  clipped.painted_pixels_clipped === 0,
  `② 裁空前提：painted_pixels_clipped=${clipped.painted_pixels_clipped}（raw=${clipped.painted_pixels}）`,
);
check(
  clipped.landed === false && clipped.landed_reason === "clipped_empty",
  `② 位图选区裁空 ⇒ landed=${clipped.landed} reason=${clipped.landed_reason}`,
);

// ③ 正常一笔
const normal = await call("brush_stroke", {
  layer_id: "layer_landed",
  brush: "calligraphy",
  points: strokePoints,
  size: 30,
  color: [0.2, 0.4, 0.9, 1.0],
});
if (normal.ok !== true) fail(`③ brush_stroke（正常）⇒ ${JSON.stringify(normal).slice(0, 160)}`);
check(
  normal.landed === true && normal.landed_reason === "painted",
  `③ 正常一笔 ⇒ landed=${normal.landed} reason=${normal.landed_reason}（clipped=${normal.painted_pixels_clipped}、delta=${normal.max_channel_delta}）`,
);

// ④ medium 正常一笔
const medium = await call("medium_stroke", {
  layer_id: "layer_landed",
  medium: "oil",
  points: [
    [40, 60, 1],
    [280, 180, 1],
  ],
  size: 40,
  load: 0.8,
  color: { r: 30, g: 60, b: 200, a: 255 },
});
if (medium.ok !== true) fail(`④ medium_stroke（正常）⇒ ${JSON.stringify(medium).slice(0, 160)}`);
check(
  medium.landed === true && medium.landed_reason === "painted",
  `④ medium 正常一笔 ⇒ landed=${medium.landed} reason=${medium.landed_reason}`,
);
check(
  typeof medium.ink_mass === "number" && typeof medium.ink_footprint_pixels === "number",
  `④ medium 带墨量字段：ink_mass=${medium.ink_mass}、ink_footprint_pixels=${medium.ink_footprint_pixels}`,
);

// ⑤ medium（`pixel` 品类）白底画白墨 ⇒ 什么都没落下（`painted=0` ⇒ clipped=0）。
//    品类必须用 `pixel`：`oil`/`watercolor`/`marker` 的白落到画布上不是白
//    （介质模拟改色，实测画布最大 RGB 差 53 ⇒ 真可见 ⇒ `landed:true` 才对）。
//    必须用**独立文档**：①–④ 已在主文档上落了墨，共用文档会让 pixel 白笔的底色
//    不再是白（916 轮教训：多 case 共用一文档 ⇒ 前一段的墨污染后一段的量）。
const pixelDocId = `${doc}-pixel`;
const pixelDoc = await (
  await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: pixelDocId, width: 320, height: 240 }),
  })
).json();
if (!pixelDoc.token) {
  console.log("  ⊘ 不可判：建不了 ⑤ 臂的独立文档");
  process.exit(3);
}
const callPixel = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(pixelDocId)}&token=${encodeURIComponent(pixelDoc.token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const pixelLayer = await callPixel("create_layer", { layer_id: "layer_pixel", name: "pixel" });
if (pixelLayer.ok !== true) fail(`⑤ create_layer ⇒ ${JSON.stringify(pixelLayer).slice(0, 120)}`);
const mediumWhite = await callPixel("medium_stroke", {
  layer_id: "layer_pixel",
  medium: "pixel",
  points: [
    [40, 60, 1],
    [280, 180, 1],
  ],
  size: 40,
  load: 0.8,
  color: { r: 255, g: 255, b: 255, a: 255 },
});
if (mediumWhite.ok !== true) fail(`⑤ medium_stroke（pixel 白墨）⇒ ${JSON.stringify(mediumWhite).slice(0, 160)}`);
if (!(mediumWhite.ink_footprint_pixels > 0)) {
  console.log("  ⊘ 不可判：medium 白墨没落足迹 ⇒ 测的不是 medium 落空那支");
  process.exit(3);
}
check(
  mediumWhite.painted_pixels === 0,
  `⑤ 落空前提：painted_pixels=${mediumWhite.painted_pixels}（足迹 ${mediumWhite.ink_footprint_pixels}、delta=${mediumWhite.paint_delta}）`,
);
check(
  mediumWhite.landed === false && mediumWhite.landed_reason === "clipped_empty",
  `⑤ medium 白上画白（pixel）⇒ landed=${mediumWhite.landed} reason=${mediumWhite.landed_reason}`,
);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（landed 顶层结论）`);
  process.exit(1);
}
console.log("\n  ✓ landed 顶层结论：白上画白 / 裁空 / 正常三臂 + medium 两臂全对");
