#!/usr/bin/env node
// **`ink_mass` 必须是真墨量，不是足迹数**（A2）
//
// **⇒ 问题**：`ink_footprint_pixels` 数的是"alpha 非零的像素数" ⇒ 同一支笔四种浓度，
//   足迹只降 3.3% 而 `max_channel_delta` 从 221 掉到 74（D193）⇒ 透明墨与浓墨读出来
//   同一个数，调用方分不清。
//
// **判据**：
//   ① 同一支笔四档浓度（opacity 1.0 / 0.5 / 0.2 / 0.05）⇒ `ink_mass` 严格单调递减；
//   ② 墨量比值落在合理区间：`ink_mass(0.05) / ink_mass(1.0) < 0.5`
//      （足迹在同一对比里只降百分之几 ⇒ 这一条正是"墨量 ≠ 足迹"的分水岭）；
//   ③ 恒等式 `0 ≤ ink_mass ≤ ink_footprint_pixels × 255` 在各档 + 两笔随机笔下恒成立；
//   ④ `ink_mass` 与 `ink_footprint_pixels` 不恒等（0.5 档上两者数值不同）。
//
// **变异**：墨量改成足迹数 ⇒ ② 红（足迹比值 ≈ 1，不进 0.5）；恒等式改成 `≤ footprint` ⇒ ③ 红 ✓。
// **判据自身出错 ⇒ 退出码 3（不是产品失败）** ✓（第 1132 轮整合时补）：
//   ⇒ 本判据用了 `fetch(` ⇒ 服务没起时异常会以**退出码 1** 冒出去 ✓
//   ⇒ 那与"产品失败"同码 ⇒ `check-criteria-have-exit3` 会红 ✓（整合时实测：本文件缺它）。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)} ⇒ 退出码 3`);
  process.exit(3);
});
process.on("unhandledRejection", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)} ⇒ 退出码 3`);
  process.exit(3);
});
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-ink-mass.mjs <base> <doc> <token>");
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

const layer = await call("create_layer", { layer_id: "layer_inkmass", name: "inkmass" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
const stroke = async (opacity, size = 30) =>
  call("brush_stroke", {
    layer_id: "layer_inkmass",
    brush: "calligraphy",
    points: [
      [40, 60, 0.8],
      [280, 180, 0.8],
    ],
    size,
    opacity,
    color: [0.2, 0.4, 0.9, 1.0],
  });

// ①+②：四档浓度同一支笔
const opacities = [1.0, 0.5, 0.2, 0.05];
const masses = [];
const footprints = [];
for (const opacity of opacities) {
  const r = await stroke(opacity);
  if (r.ok !== true) fail(`opacity=${opacity} ⇒ brush_stroke 失败 ${JSON.stringify(r).slice(0, 160)}`);
  if (typeof r.ink_mass !== "number" || typeof r.ink_footprint_pixels !== "number") {
    fail(`opacity=${opacity} ⇒ 缺墨量字段（ink_mass=${r.ink_mass}、ink_footprint_pixels=${r.ink_footprint_pixels}）`);
  }
  masses.push(r.ink_mass);
  footprints.push(r.ink_footprint_pixels);
  console.log(`  opacity=${opacity} ⇒ ink_mass=${r.ink_mass}、footprint=${r.ink_footprint_pixels}`);
}
const strictlyDecreasing = masses.every((m, i) => i === 0 || m < masses[i - 1]);
check(
  strictlyDecreasing,
  `① 四档浓度 ink_mass 严格单调递减：${masses.join(" > ")}`,
);
const ratio = masses[3] / masses[0];
check(
  ratio < 0.5,
  `② 墨量比值 ink_mass(0.05)/ink_mass(1.0)=${ratio.toFixed(3)} < 0.5（足迹同口径比值 ${(
    footprints[3] / footprints[0]
  ).toFixed(3)} ⇒ 墨量与足迹不是同一个量）`,
);

// ③：恒等式在各档 + 两笔"随机"笔下恒成立
const identity = (inkMass, footprint, tag) =>
  check(
    Number.isInteger(inkMass) && inkMass >= 0 && inkMass <= footprint * 255,
    `③ 恒等式 0 ≤ ink_mass ≤ footprint×255（${tag}：${inkMass} ≤ ${footprint}×255=${footprint * 255}）`,
  );
opacities.forEach((opacity, i) => identity(masses[i], footprints[i], `opacity=${opacity}`));
for (const [size, opacity] of [
  [60, 0.3],
  [14, 0.7],
]) {
  const r = await stroke(opacity, size);
  if (r.ok !== true) fail(`随机笔 size=${size} opacity=${opacity} ⇒ 失败 ${JSON.stringify(r).slice(0, 120)}`);
  identity(r.ink_mass, r.ink_footprint_pixels, `size=${size} opacity=${opacity}`);
}

// ④：墨量与足迹不恒等（0.5 档上两者数值必须不同，否则墨量字段就是足迹的复读）
check(
  masses[1] !== footprints[1],
  `④ ink_mass ≠ ink_footprint_pixels（0.5 档：${masses[1]} ≠ ${footprints[1]}）`,
);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（ink_mass 真墨量）`);
  process.exit(1);
}
console.log("\n  ✓ ink_mass：四档单调 + 比值分水岭 + 恒等式恒成立 + 非足迹复读");
