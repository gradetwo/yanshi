#!/usr/bin/env node
// **C1：`color_space` 显式颜色单位**（`brush_stroke` / `medium_stroke`）：
//   新参数 `color_space: "srgb" | "linear"` 作用于 `color` 与 `color_to`；
//   不传 ⇒ 现行量级切换原样保留（浮点数组 = 线性，任一分量 > 1 = sRGB 字节）；
//   `"srgb"` ⇒ 0..1 浮点数组按 sRGB 字节解释（`v*255` 取整再解码）；
//   `"linear"` ⇒ 浮点数组按线性直通（即使分量 > 1 也不 reroute 到字节分支）；
//   对象式 / hex 不受参数影响（恒为字节）。
//   回执 `applied.color_space` 回显实际生效的单位：`"linear"` / `"srgb_bytes"` / `"hex"`。
//
// **判据**（每臂独立文档，免得前一笔污染后一臂的采样；`100%_Opaque` 全不透明 ⇒
//   采样值就是笔色本身，探针实测浮点 [0.45,0.38,0.30] ⇒ (179,166,149)、
//   字节 [115,97,77] ⇒ (115,97,77)，逐点一致）：
//   ① brush `color:[0.45,0.38,0.30]` + `color_space:"srgb"` ⇒ 显示即 (115,97,77)（±3），
//      且 `applied.color_space == "srgb_bytes"`；
//   ② 同色字节臂 `[115,97,77]`（不传参数）⇒ 与 ① 逐通道 |Δ| ≤ 3（同一条字节管线）；
//   ③ 不传 `color_space` 的浮点臂 ⇒ 显示 (179,166,149)（±3），回显 `"linear"`（缺省不变）；
//   ④ `color_space:"linear"` + 同数组 ⇒ 与 ③ 逐通道 |Δ| ≤ 3，回显 `"linear"`；
//   ⑤ 非法值 `"lab"` ⇒ `ok:false` + `invalid_argument`，报错里列出两个允许值；
//   ⑥ medium_stroke：srgb 浮点臂与字节臂中位色 |Δ| ≤ 8，回显 `"srgb_bytes"`；
//      线性浮点臂红通道比 srgb 臂高 ≥ 20，回显 `"linear"`；
//   ⑦ hex + 显式 `"srgb"` ⇒ 回显仍是 `"hex"`；对象式 + 显式 `"linear"` ⇒ 回显仍是 `"srgb_bytes"`
//     （对象式 / hex 不受参数影响）；
//   ⑧ `color_to` 也认 `color_space`：srgb 渐变末端绿通道 < 60，缺省渐变末端绿通道 > 70。
//
// **变异**：`color_space` 参数被忽略（总走量级切换）⇒ ①⑥⑧ 红；
//   缺省改成 srgb（浮点一律按字节解释）⇒ ③④ 红；
//   回显恒写 `"srgb_bytes"` ⇒ ③④⑥ 的线性臂红；
//   `color_to` 不认参数 ⇒ ⑧ 的 srgb 臂红。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-color-space.mjs <base-url>"); process.exit(2); }

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

const brushArm = async (doc, args) => {
  const token = await mkdoc(doc);
  await call(doc, token, "create_layer", { layer_id: "L", name: "l" });
  const r = await call(doc, token, "brush_stroke", {
    layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30, ...args,
  });
  if (r.ok !== true) fail(`${doc} brush_stroke 失败 ⇒ ${JSON.stringify(r).slice(0, 160)}`);
  const sample = await sampleMedian(doc, token, samplePoints);
  return { r, sample };
};

// ① srgb 浮点臂
const srgb = await brushArm(`cs-srgb-${process.pid}`, { color: [0.45, 0.38, 0.30], color_space: "srgb" });
check(srgb.r.applied?.color_space === "srgb_bytes", `① srgb 浮点臂回显 color_space=${srgb.r.applied?.color_space}（应为 srgb_bytes）`);
check(close(srgb.sample, [115, 97, 77], 3), `① srgb [0.45,0.38,0.30] 显示=${JSON.stringify(srgb.sample)}（应为 115,97,77 ±3）`);
// ② 字节臂（同一条字节管线 ⇒ 与 ① 逐通道一致）
const byte = await brushArm(`cs-byte-${process.pid}`, { color: [115, 97, 77] });
check(close(byte.sample, [115, 97, 77], 3), `② 字节 [115,97,77] 显示=${JSON.stringify(byte.sample)}（应为 115,97,77 ±3）`);
check(close(srgb.sample, byte.sample, 3), `② srgb 浮点臂与字节臂逐通道 |Δ|≤3（实测 ${JSON.stringify(srgb.sample)} vs ${JSON.stringify(byte.sample)}）`);
// ③ 缺省浮点臂（量级切换原样保留 ⇒ 线性）
const def = await brushArm(`cs-def-${process.pid}`, { color: [0.45, 0.38, 0.30] });
check(def.r.applied?.color_space === "linear", `③ 不传 color_space 回显 color_space=${def.r.applied?.color_space}（应为 linear）`);
check(close(def.sample, [179, 166, 149], 3), `③ 缺省浮点 [0.45,0.38,0.30] 显示=${JSON.stringify(def.sample)}（应为 179,166,149 ±3，缺省仍是线性）`);
// ④ 线性显式臂（与缺省同一条路）
const lin = await brushArm(`cs-lin-${process.pid}`, { color: [0.45, 0.38, 0.30], color_space: "linear" });
check(lin.r.applied?.color_space === "linear", `④ linear 显式臂回显 color_space=${lin.r.applied?.color_space}（应为 linear）`);
check(close(lin.sample, def.sample, 3), `④ linear 显式臂与缺省臂逐通道 |Δ|≤3（实测 ${JSON.stringify(lin.sample)} vs ${JSON.stringify(def.sample)}）`);
// ⑤ 非法值
const badDoc = `cs-bad-${process.pid}`;
const badToken = await mkdoc(badDoc);
await call(badDoc, badToken, "create_layer", { layer_id: "L", name: "l" });
const bad = await call(badDoc, badToken, "brush_stroke", {
  layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
  color: [0.45, 0.38, 0.30], color_space: "lab",
});
check(bad.ok === false, `⑤ color_space:"lab" ⇒ ok=${bad.ok}（应为 false）`);
check(bad.error_code === "invalid_argument", `⑤ 报错码=${bad.error_code}（应为 invalid_argument）`);
const detail = JSON.stringify(bad.context?.detail ?? bad);
check(detail.includes("srgb") && detail.includes("linear"), `⑤ 报错里列出两个允许值（实测 ${detail.slice(0, 120)}）`);

// ⑥ medium_stroke 两臂
const mediumArm = async (doc, args) => {
  const token = await mkdoc(doc);
  await call(doc, token, "create_layer", { layer_id: "L", name: "l" });
  const r = await call(doc, token, "medium_stroke", {
    layer_id: "L", medium: "oil", points: linePoints, size: 40, load: 1.0, ...args,
  });
  if (r.ok !== true) fail(`${doc} medium_stroke 失败 ⇒ ${JSON.stringify(r).slice(0, 160)}`);
  const sample = await sampleMedian(doc, token, samplePoints);
  return { r, sample };
};
const medSrgb = await mediumArm(`cs-med-srgb-${process.pid}`, { color: [0.45, 0.38, 0.30], color_space: "srgb" });
const medByte = await mediumArm(`cs-med-byte-${process.pid}`, { color: [115, 97, 77] });
const medLin = await mediumArm(`cs-med-lin-${process.pid}`, { color: [0.45, 0.38, 0.30], color_space: "linear" });
check(medSrgb.r.applied?.color_space === "srgb_bytes", `⑥ medium srgb 浮点臂回显=${medSrgb.r.applied?.color_space}（应为 srgb_bytes）`);
check(close(medSrgb.sample, medByte.sample, 8), `⑥ medium srgb 浮点臂与字节臂中位色 |Δ|≤8（实测 ${JSON.stringify(medSrgb.sample)} vs ${JSON.stringify(medByte.sample)}）`);
check(medLin.r.applied?.color_space === "linear", `⑥ medium linear 臂回显=${medLin.r.applied?.color_space}（应为 linear）`);
check(
  medLin.sample && medSrgb.sample && medLin.sample[0] - medSrgb.sample[0] >= 20,
  `⑥ medium 线性臂红通道比 srgb 臂高 ≥20（实测 ${medLin.sample?.[0]} vs ${medSrgb.sample?.[0]}）`,
);

// ⑦ 对象式 / hex 不受参数影响（只看回显，不画第二笔）
const hexDoc = `cs-hex-${process.pid}`;
const hexToken = await mkdoc(hexDoc);
await call(hexDoc, hexToken, "create_layer", { layer_id: "L", name: "l" });
const hex = await call(hexDoc, hexToken, "brush_stroke", {
  layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
  color: "#73391F", color_space: "srgb",
});
check(hex.ok === true, `⑦ hex + 显式 srgb 仍接受（ok=${hex.ok}）`);
check(hex.applied?.color_space === "hex", `⑦ hex + 显式 srgb 回显=${hex.applied?.color_space}（应为 hex，hex 不受参数影响）`);
const obj = await call(hexDoc, hexToken, "brush_stroke", {
  layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
  color: { r: 115, g: 97, b: 77 }, color_space: "linear",
});
check(obj.ok === true, `⑦ 对象式 + 显式 linear 仍接受（ok=${obj.ok}）`);
check(obj.applied?.color_space === "srgb_bytes", `⑦ 对象式 + 显式 linear 回显=${obj.applied?.color_space}（应为 srgb_bytes，对象式恒为字节）`);

// ⑧ color_to 也认 color_space（渐变末端绿通道分离）
const gradEnd = async (doc, extra) => {
  const token = await mkdoc(doc);
  await call(doc, token, "create_layer", { layer_id: "L", name: "l" });
  const r = await call(doc, token, "brush_stroke", {
    layer_id: "L", brush: "100%_Opaque", points: linePoints, size: 30,
    color: [0.45, 0.38, 0.30], color_to: [0.9, 0.1, 0.1], ...extra,
  });
  if (r.ok !== true) fail(`${doc} 渐变笔失败 ⇒ ${JSON.stringify(r).slice(0, 160)}`);
  const px = await call(doc, token, "sample_colors", { points: [[250, 120], [265, 120], [272, 120]] });
  const greens = (px.samples ?? []).map((s) => s.rgba?.[1]).filter(Number.isFinite);
  return Math.min(...greens);
};
const gradSrgb = await gradEnd(`cs-grad-srgb-${process.pid}`, { color_space: "srgb" });
const gradDef = await gradEnd(`cs-grad-def-${process.pid}`, {});
check(gradSrgb < 60, `⑧ srgb 渐变末端绿通道=${gradSrgb}（应 <60，末端色按 sRGB 字节解释）`);
check(gradDef > 70, `⑧ 缺省渐变末端绿通道=${gradDef}（应 >70，末端色按线性解释）`);

if (failures.length) {
  console.log(`\n  ✗ ${failures.length} 处不符（C1 color_space）`);
  process.exit(1);
}
console.log("\n  ✓ C1 color_space：srgb/linear/缺省三路 + 非法值 + medium + 对象/hex 不受影响 + color_to 全对");
