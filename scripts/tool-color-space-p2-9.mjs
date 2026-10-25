#!/usr/bin/env node
// **P2-9：`color_space` 补齐三路**（`try_stroke` / `brush_preview` / `update_stroke`）：
//   与期 1 C1（`brush_stroke` / `medium_stroke`）同口径 —— 显式参数只认 `"srgb"` / `"linear"`，
//   不传 ⇒ 现行量级切换原样保留；非法值 ⇒ `invalid_argument`；回执 `applied.color_space`
//   回显实际生效的单位（`"linear"` / `"srgb_bytes"` / `"hex"`）。
//   `brush_preview` 顶层 `color_space` 仍是混色与输出的空间（恒 `"srgb"`），不要与
//   `applied.color_space`（输入色的解释空间）混淆。
//
// **判据**（每臂独立文档，免得前一笔污染后一臂的采样；`100%_Opaque` 全不透明 ⇒
//   采样值就是笔色本身；浮点 [0.45,0.38,0.30] ⇒ 线性 (179,166,149)、sRGB 字节 (115,97,77)）：
//   ① try_stroke srgb 浮点臂 ⇒ `applied.color_space == "srgb_bytes"`，commit 后采样 (115,97,77) ±3；
//   ② try_stroke 缺省浮点臂 ⇒ 回显 `"linear"`，commit 后采样 (179,166,149) ±3；
//   ③ try_stroke linear 显式臂 ⇒ 回显 `"linear"`，采样与 ② 逐通道 |Δ| ≤ 3；
//   ④ try_stroke 非法值 `"lab"` ⇒ `ok:false` + `invalid_argument`，报错列出两个允许值；
//   ⑤ try_stroke `based_on` 继承空间：首笔 srgb，次笔只换色不给空间 ⇒ 回显仍 `"srgb_bytes"`，
//      commit 后采样仍 (115,97,77) ±3；
//   ⑥ brush_preview srgb 臂 ⇒ 回显 `"srgb_bytes"`，`max_channel_delta` ≈ 178（白底，255−77），
//      顶层 `color_space` 仍是 `"srgb"`；
//   ⑦ brush_preview linear 臂 ⇒ 回显 `"linear"`，`max_channel_delta` ≈ 106（255−149），
//      与 ⑥ 差 ≥ 50；
//   ⑧ brush_preview 缺省臂 ⇒ 回显 `"linear"`，delta ≈ ⑦（±5）；
//   ⑨ brush_preview 非法值 ⇒ `ok:false` + `invalid_argument`；
//   ⑩ update_stroke 光栅重跑：缺省浮点笔 (179,166,149) ⇒ 换色 + `color_space:"srgb"` ⇒
//      新 blob 哈希 ≠ 旧哈希，且 `source.color_space == "srgb"`（画布像素不断：光栅重跑像素
//      不落地是既有已知红，Rust 测试 `update_stroke_recolours_a_brush_stroke` 已 #[ignore]）；
//   ⑪ 同色换 `color_space:"linear"` ⇒ blob 哈希 ≠ srgb 臂（两空间解释出的字节不同），
//      `source.color_space == "linear"`；
//   ⑫ 再换色不给空间 ⇒ `source.color_space` 仍是 `"linear"`，blob 哈希与 linear 臂相同
//      （重跑确定性 + 空间持久化）；
//   ⑬ update_stroke 非法值 ⇒ `ok:false` + `invalid_argument`；
//   ⑭ update_stroke 矢量笔迹（draw_stroke）：缺省浮点色 (179,166,149) ⇒
//      换色 + `color_space:"srgb"` ⇒ 采样 (115,97,77) ±3，且 `data.color_space == "srgb"`；
//      只换空间回 `"linear"`（不换颜色）⇒ 采样回到 (179,166,149) ±3（空间是笔的配方）。
//
// **变异**：三路任一路忽略参数（总走量级切换）⇒ 对应 srgb 臂红；
//   缺省改成 srgb ⇒ ②⑧⑩ 的缺省臂红；回显恒写 `"srgb_bytes"` ⇒ ②③⑦⑧ 的线性臂红；
//   重跑不把新空间写进 source ⇒ ⑫ 红；矢量路径不读 `data.color_space` ⇒ ⑭ 红。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-color-space-p2-9.mjs <base-url>"); process.exit(2); }

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
const call = (doc, token, tool, args) => fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
}).then((r) => r.json());

const failures = [];
const check = (ok, msg) => {
  if (!ok) failures.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};
const fail = (message) => { console.error(`  ✗ ${message}`); process.exit(1); };
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const sampleMedian = async (doc, token, points) => {
  const px = await call(doc, token, "sample_colors", { points });
  const samples = px.samples?.map((s) => s.rgba) ?? [];
  if (samples.length !== points.length) return null;
  return [0, 1, 2].map((c) => median(samples.map((s) => s[c])));
};
const close = (got, want, tol) =>
  got && want.every((w, i) => Math.abs(got[i] - w) <= tol);
const linePoints = [[40, 120, 1.0], [280, 120, 1.0]];
const samplePoints = [[100, 120], [160, 120], [220, 120]];
const FLOAT_COLOR = [0.45, 0.38, 0.30];
const SRGB_BYTES = [115, 97, 77];
const LINEAR_BYTES = [179, 166, 149];

const tryArm = async (doc, args) => {
  const token = await mkdoc(doc);
  await call(doc, token, "create_layer", { layer_id: "L", name: "l" });
  const r = await call(doc, token, "try_stroke", {
    layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
    color: FLOAT_COLOR, ...args,
  });
  if (r.ok !== true) fail(`${doc} try_stroke 失败 ⇒ ${JSON.stringify(r).slice(0, 160)}`);
  const committed = await call(doc, token, "commit_stroke", { attempt_id: r.attempt_id });
  if (committed.ok !== true) fail(`${doc} commit_stroke 失败 ⇒ ${JSON.stringify(committed).slice(0, 160)}`);
  const sample = await sampleMedian(doc, token, samplePoints);
  return { r, sample };
};

// ① try_stroke srgb 浮点臂
const tSrgb = await tryArm(`cs9-try-srgb-${process.pid}`, { color_space: "srgb" });
check(tSrgb.r.applied?.color_space === "srgb_bytes", `① try srgb 浮点臂回显 color_space=${tSrgb.r.applied?.color_space}（应为 srgb_bytes）`);
check(close(tSrgb.sample, SRGB_BYTES, 3), `① try srgb [0.45,0.38,0.30] commit 后显示=${JSON.stringify(tSrgb.sample)}（应为 115,97,77 ±3）`);
// ② try_stroke 缺省浮点臂
const tDef = await tryArm(`cs9-try-def-${process.pid}`, {});
check(tDef.r.applied?.color_space === "linear", `② try 不传 color_space 回显 color_space=${tDef.r.applied?.color_space}（应为 linear）`);
check(close(tDef.sample, LINEAR_BYTES, 3), `② try 缺省浮点 commit 后显示=${JSON.stringify(tDef.sample)}（应为 179,166,149 ±3，缺省仍是线性）`);
// ③ try_stroke linear 显式臂
const tLin = await tryArm(`cs9-try-lin-${process.pid}`, { color_space: "linear" });
check(tLin.r.applied?.color_space === "linear", `③ try linear 显式臂回显 color_space=${tLin.r.applied?.color_space}（应为 linear）`);
check(close(tLin.sample, tDef.sample, 3), `③ try linear 显式臂与缺省臂逐通道 |Δ|≤3（实测 ${JSON.stringify(tLin.sample)} vs ${JSON.stringify(tDef.sample)}）`);
// ④ try_stroke 非法值
const badDoc = `cs9-try-bad-${process.pid}`;
const badToken = await mkdoc(badDoc);
await call(badDoc, badToken, "create_layer", { layer_id: "L", name: "l" });
const bad = await call(badDoc, badToken, "try_stroke", {
  layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
  color: FLOAT_COLOR, color_space: "lab",
});
check(bad.ok === false, `④ try color_space:"lab" ⇒ ok=${bad.ok}（应为 false）`);
check(bad.error_code === "invalid_argument", `④ try 报错码=${bad.error_code}（应为 invalid_argument）`);
const badDetail = JSON.stringify(bad.context?.detail ?? bad);
check(badDetail.includes("srgb") && badDetail.includes("linear"), `④ try 报错里列出两个允许值（实测 ${badDetail.slice(0, 120)}）`);
// ⑤ try_stroke based_on 继承空间
const inhDoc = `cs9-try-inh-${process.pid}`;
const inhToken = await mkdoc(inhDoc);
await call(inhDoc, inhToken, "create_layer", { layer_id: "L", name: "l" });
const first = await call(inhDoc, inhToken, "try_stroke", {
  layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
  color: FLOAT_COLOR, color_space: "srgb",
});
if (first.ok !== true) fail(`⑤ 首笔 try_stroke 失败 ⇒ ${JSON.stringify(first).slice(0, 160)}`);
const second = await call(inhDoc, inhToken, "try_stroke", {
  based_on: first.attempt_id, color: FLOAT_COLOR,
});
check(second.ok === true, `⑤ based_on 次笔仍接受（ok=${second.ok}）`);
check(second.applied?.color_space === "srgb_bytes", `⑤ based_on 次笔回显 color_space=${second.applied?.color_space}（应为 srgb_bytes，空间随 based_on 继承）`);
const committed2 = await call(inhDoc, inhToken, "commit_stroke", { attempt_id: second.attempt_id });
if (committed2.ok !== true) fail(`⑤ 次笔 commit 失败 ⇒ ${JSON.stringify(committed2).slice(0, 160)}`);
const inhSample = await sampleMedian(inhDoc, inhToken, samplePoints);
check(close(inhSample, SRGB_BYTES, 3), `⑤ based_on 次笔 commit 后显示=${JSON.stringify(inhSample)}（应为 115,97,77 ±3）`);

// ⑥⑦⑧ brush_preview 三臂（max_channel_delta 读数：白底；srgb (115,97,77) ⇒ 178，线性 (179,166,149) ⇒ 106）
const previewArm = async (doc, args) => {
  const token = await mkdoc(doc);
  await call(doc, token, "create_layer", { layer_id: "L", name: "l" });
  const r = await call(doc, token, "brush_preview", {
    brush: "100%_Opaque", points: linePoints, size: 30, color: FLOAT_COLOR, ...args,
  });
  if (r.ok !== true) fail(`${doc} brush_preview 失败 ⇒ ${JSON.stringify(r).slice(0, 160)}`);
  return r;
};
const pSrgb = await previewArm(`cs9-pv-srgb-${process.pid}`, { color_space: "srgb" });
check(pSrgb.applied?.color_space === "srgb_bytes", `⑥ preview srgb 臂回显=${pSrgb.applied?.color_space}（应为 srgb_bytes）`);
check(pSrgb.color_space === "srgb", `⑥ preview 顶层 color_space=${pSrgb.color_space}（应仍是 srgb：混色与输出的空间）`);
check(Math.abs((pSrgb.max_channel_delta ?? -1) - 178) <= 5, `⑥ preview srgb 臂 max_channel_delta=${pSrgb.max_channel_delta}（应 ≈178，白底 255−77）`);
const pLin = await previewArm(`cs9-pv-lin-${process.pid}`, { color_space: "linear" });
check(pLin.applied?.color_space === "linear", `⑦ preview linear 臂回显=${pLin.applied?.color_space}（应为 linear）`);
check(Math.abs((pLin.max_channel_delta ?? -1) - 106) <= 5, `⑦ preview linear 臂 max_channel_delta=${pLin.max_channel_delta}（应 ≈106，白底 255−149）`);
check((pSrgb.max_channel_delta ?? 0) - (pLin.max_channel_delta ?? 0) >= 50, `⑦ preview 两臂 delta 差 ≥50（实测 ${pSrgb.max_channel_delta} vs ${pLin.max_channel_delta}）`);
const pDef = await previewArm(`cs9-pv-def-${process.pid}`, {});
check(pDef.applied?.color_space === "linear", `⑧ preview 缺省臂回显=${pDef.applied?.color_space}（应为 linear）`);
check(Math.abs((pDef.max_channel_delta ?? -1) - (pLin.max_channel_delta ?? -1)) <= 5, `⑧ preview 缺省臂 delta ≈ linear 臂（实测 ${pDef.max_channel_delta} vs ${pLin.max_channel_delta}）`);
// ⑨ brush_preview 非法值
const pvBadDoc = `cs9-pv-bad-${process.pid}`;
const pvBadToken = await mkdoc(pvBadDoc);
await call(pvBadDoc, pvBadToken, "create_layer", { layer_id: "L", name: "l" });
const pvBad = await call(pvBadDoc, pvBadToken, "brush_preview", {
  brush: "100%_Opaque", points: linePoints, size: 30, color: FLOAT_COLOR, color_space: "lab",
});
check(pvBad.ok === false, `⑨ preview color_space:"lab" ⇒ ok=${pvBad.ok}（应为 false）`);
check(pvBad.error_code === "invalid_argument", `⑨ preview 报错码=${pvBad.error_code}（应为 invalid_argument）`);

// ⑩⑪⑫ update_stroke 光栅重跑（三臂同一文档：换色+空间 → 同色换空间 → 再换色不给空间）
//   **为什么不断画布像素**：光栅重跑的像素不落地是既有已知红（Rust 测试
//   `update_stroke_recolours_a_brush_stroke` 已 #[ignore]：换色后画布仍是旧色，
//   红 3662 / 蓝 0，本期不修）⇒ 光栅臂断"重跑产出了什么"（blob 哈希 + source 记录），
//   像素断言只放在矢量臂 ⑭（矢量路径渲染正常）。
const upDoc = `cs9-up-${process.pid}`;
const upToken = await mkdoc(upDoc);
await call(upDoc, upToken, "create_layer", { layer_id: "L", name: "l" });
const painted = await call(upDoc, upToken, "brush_stroke", {
  layer_id: "L", object_id: "B1", brush: "100%_Opaque", points: linePoints, size: 30, color: FLOAT_COLOR,
});
if (painted.ok !== true) fail(`⑩ brush_stroke 失败 ⇒ ${JSON.stringify(painted).slice(0, 160)}`);
const upBefore = await sampleMedian(upDoc, upToken, samplePoints);
check(close(upBefore, LINEAR_BYTES, 3), `⑩ 缺省浮点笔显示=${JSON.stringify(upBefore)}（应为 179,166,149 ±3）`);
const gotBefore = await call(upDoc, upToken, "get_object", { object_id: "B1" });
const oldHash = gotBefore.data?.bitmap?.blob_hash;
const up1 = await call(upDoc, upToken, "update_stroke", {
  object_id: "B1", core: { color: FLOAT_COLOR, color_space: "srgb" },
});
check(up1.ok === true, `⑩ update 换色+srgb 接受（ok=${up1.ok}）`);
const got1 = await call(upDoc, upToken, "get_object", { object_id: "B1" });
const srgbHash = got1.data?.bitmap?.blob_hash;
check(!!srgbHash && srgbHash !== oldHash, `⑩ 换色+srgb 产出新 blob（重跑真的跑了）`);
check(got1.data?.source?.color_space === "srgb", `⑩ source.color_space=${got1.data?.source?.color_space}（应为 srgb）`);
const up2b = await call(upDoc, upToken, "update_stroke", {
  object_id: "B1", core: { color: FLOAT_COLOR, color_space: "linear" },
});
check(up2b.ok === true, `⑪ 同色换 linear 接受（ok=${up2b.ok}）`);
const got2 = await call(upDoc, upToken, "get_object", { object_id: "B1" });
const linearHash = got2.data?.bitmap?.blob_hash;
check(!!linearHash && linearHash !== srgbHash, `⑪ 同色不同空间 ⇒ blob 必须不同（否则空间没生效）`);
check(got2.data?.source?.color_space === "linear", `⑪ source.color_space=${got2.data?.source?.color_space}（应为 linear）`);
const up4 = await call(upDoc, upToken, "update_stroke", {
  object_id: "B1", core: { color: FLOAT_COLOR },
});
check(up4.ok === true, `⑫ 再换色不给空间接受（ok=${up4.ok}）`);
const got4 = await call(upDoc, upToken, "get_object", { object_id: "B1" });
check(got4.data?.source?.color_space === "linear", `⑫ 再换色不给空间，source.color_space=${got4.data?.source?.color_space}（应仍为 linear）`);
check(got4.data?.bitmap?.blob_hash === linearHash, `⑫ 同色同空间重跑 ⇒ blob 应与 linear 臂相同（重跑是确定性的）`);
// ⑬ update_stroke 非法值
const upBad = await call(upDoc, upToken, "update_stroke", {
  object_id: "B1", core: { color_space: "lab" },
});
check(upBad.ok === false, `⑬ update color_space:"lab" ⇒ ok=${upBad.ok}（应为 false）`);
check(upBad.error_code === "invalid_argument", `⑬ update 报错码=${upBad.error_code}（应为 invalid_argument）`);

// ⑭ update_stroke 矢量笔迹（draw_stroke）
const vecDoc = `cs9-vec-${process.pid}`;
const vecToken = await mkdoc(vecDoc);
await call(vecDoc, vecToken, "create_layer", { layer_id: "L", name: "l" });
const drawn = await call(vecDoc, vecToken, "draw_stroke", {
  layer_id: "L", object_id: "V1",
  data: { points: [[40, 120], [280, 120]], size: 30, color: FLOAT_COLOR, opacity: 1.0 },
});
if (drawn.ok !== true) fail(`⑭ draw_stroke 失败 ⇒ ${JSON.stringify(drawn).slice(0, 160)}`);
const vecBefore = await sampleMedian(vecDoc, vecToken, samplePoints);
check(close(vecBefore, LINEAR_BYTES, 3), `⑭ 矢量缺省浮点色显示=${JSON.stringify(vecBefore)}（应为 179,166,149 ±3）`);
const vecUp = await call(vecDoc, vecToken, "update_stroke", {
  object_id: "V1", core: { color: FLOAT_COLOR, color_space: "srgb" },
});
check(vecUp.ok === true, `⑭ 矢量 update 换色+srgb 接受（ok=${vecUp.ok}）`);
const vecAfter = await sampleMedian(vecDoc, vecToken, samplePoints);
check(close(vecAfter, SRGB_BYTES, 3), `⑭ 矢量换色+srgb 后显示=${JSON.stringify(vecAfter)}（应为 115,97,77 ±3）`);
const vecGot = await call(vecDoc, vecToken, "get_object", { object_id: "V1" });
check(vecGot.data?.color_space === "srgb", `⑭ 矢量 data.color_space=${vecGot.data?.color_space}（应为 srgb）`);
// 只换空间回 linear（不换颜色）⇒ 采样回到线性色（空间是笔的配方，矢量路径像素正常）
const vecUp2 = await call(vecDoc, vecToken, "update_stroke", {
  object_id: "V1", core: { color_space: "linear" },
});
check(vecUp2.ok === true, `⑭ 矢量只换空间接受（ok=${vecUp2.ok}）`);
const vecBack = await sampleMedian(vecDoc, vecToken, samplePoints);
check(close(vecBack, LINEAR_BYTES, 3), `⑭ 矢量只换空间回 linear 后显示=${JSON.stringify(vecBack)}（应为 179,166,149 ±3）`);

if (failures.length) {
  console.log(`\n  ✗ ${failures.length} 处不符（P2-9 color_space 补齐三路）`);
  process.exit(1);
}
console.log("\n  ✓ P2-9 color_space：try / preview / update 三路 srgb/linear/缺省 + 非法值 + based_on 继承 + 空间持久化全对");
