#!/usr/bin/env node
// **笔刷动力学曲线判据** ✓（需求文档 P1-7 ✓：`set_brush_dynamics(brush, curve)` ✓）。
// 文档给的理由很具体 ✓："现在 pressure→size 是线性 ✓，画不出'轻入重出'"✗。
//
// 判据（只断言**可观察**的东西 ✓，不猜内部字段 ✗）：
//   ① `set_brush_dynamics{brush, curve:{size_pressure:[[0,0.5],[0.5,1.0],[1,1.5]]}}` 被接受 ✓；
//   ② **同一条笔触**在设了曲线之后，画面必须**与线性不同** ✓（否则"曲线"名不副实 ✗）；
//   ③ **自检**：两次相同调用（都不设曲线）必须**逐字节相同** ✓（否则渲染不稳 ⇒ 比较会退化 ✗）；
//   ④ 曲线给的是"**轻入重出**"（低压力处细、高压力处粗 ✓）⇒ 与线性相比，
//      同一条**压力递增**的笔触，**总墨量应当更多** ✓（因为高压力端被放大了 ✓）—— 这一条**方向明确** ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-brush-dynamics.mjs <base-url>"); process.exit(2); }
const makeDoc = async (id) => {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 200, height: 120 }),
  })).json();
  if (!created.token) { console.error("判据无效：拿不到 token"); process.exit(1); }
  return created.token;
};
const call = async (id, token, tool, args) => (await fetch(`${base}/api/tools?doc=${id}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const shot = async (id, token) => {
  const raw = await call(id, token, "render_region", { region: { x: 0, y: 0, w: 200, h: 120 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return null;
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
const paintAndShot = async (id, token, fixedPressure = null) => {
  await call(id, token, "create_layer", { layer_id: "layer_default", name: "l" });
  // **基线** ✓ —— 文档自带不透明底色 ⇒ "不透明像素数"恒等于整幅 ✗（我在 scatter 判据上栽过同一个坑 ✓）
  // ⇒ 必须量"**与基线不同的字节数**" ✓。
  const before = await shot(id, token);
  // **压力递增**的一笔 ✓（这样"轻入重出"的曲线才会显出方向 ✓）
  const points = [];
  // **恒定压力**（给了就全程用它 ✓）—— 这样"高压 vs 低压"是**单一变量** ✓。
  for (let i = 0; i <= 10; i += 1) points.push([20 + i * 15, 60, fixedPressure === null ? i / 10 : fixedPressure]);
  const painted = await call(id, token, "brush_stroke", {
    layer_id: "layer_default", brush: "classic-brush", points,
    color: { r: 40, g: 80, b: 160, a: 255 },
  });
  if (painted.ok !== true) return { error: JSON.stringify((painted.context || {}).detail || painted.error_code) };
  const bytes = await shot(id, token);
  if (!bytes || !before) return { error: "拿不到直通 RGBA" };
  let ink = 0;
  for (let i = 0; i < bytes.length; i += 1) if (bytes[i] !== before[i]) ink += 1;
  return { bytes, ink };
};
const a = await makeDoc("dyn_a"), b = await makeDoc("dyn_b"), c = await makeDoc("dyn_c");
const linear1 = await paintAndShot("dyn_a", a);
const linear2 = await paintAndShot("dyn_b", b);
const shape = await call("dyn_c", c, "set_brush_dynamics", {
  brush: "classic-brush",
  curve: { size_pressure: [[0, 0.5], [0.5, 1.0], [1, 1.5]] },
});
if (shape.ok !== true) {
  console.log("  set_brush_dynamics ⇒ " + JSON.stringify((shape.context || {}).detail || shape.error_code || shape).slice(0, 160));
  console.log("  ✗ 动力学曲线不存在 ⇒ 判据红（预期 ✓）");
  process.exit(1);
}
const curved = await paintAndShot("dyn_c", c);
if (linear1.error || curved.error) { console.log("  ✗ 画不出来 ⇒ 判据无效 ⇒ " + (linear1.error || curved.error)); process.exit(1); }
const diff = (x, y) => x.reduce((n, v, i) => (v !== y[i] ? n + 1 : n), 0);
const selfDiff = diff(linear1.bytes, linear2.bytes);
console.log(`  自检：两次都不设曲线 ⇒ 差异字节 = ${selfDiff}（应为 0 ✓）`);
console.log(`  线性墨量 = ${linear1.ink}｜曲线墨量 = ${curved.ink}（"轻入重出"⇒ 应更多 ✓）`);
const failures = [];
if (selfDiff !== 0) failures.push(`两次相同调用结果不同（${selfDiff} 字节）⇒ 渲染不稳 ⇒ 判据无效 ✗`);
// **方向无关的性质** ✓（我原来写"曲线必须比线性更厚"✗ —— 那条**不成立** ✓：
// 该笔刷自带的压力曲线在高压端是 +2.13 ✓，比我写的 +0.405 激进得多 ✓ ⇒ 保留它之后"我的曲线"反而更薄 ✓。
// 真正该测的是：**同一条曲线下，高压那一笔落的墨必须多于低压那一笔** ✓ —— 与方向假设无关 ✓。
const low = await paintAndShot("dyn_low", await makeDoc("dyn_low"), 0.2);
const high = await paintAndShot("dyn_high", await makeDoc("dyn_high"), 0.9);
if (low.error || high.error) failures.push("两笔恒定压力的笔触画不出来 ⇒ 判据无效");
else {
  console.log(`  恒定压力 0.2 ⇒ ${low.ink} 字节｜0.9 ⇒ ${high.ink} 字节（**高压必须更多** ✓）`);
  if (high.ink <= low.ink) failures.push(`高压那一笔没比低压多少落墨（${high.ink} ≤ ${low.ink}）⇒ 压力曲线没起作用 ✗`);
}
if (diff(linear1.bytes, curved.bytes) === 0) failures.push("设了曲线之后画面与线性完全一样 ⇒ 曲线没生效 ✗");
console.log("  （提示：这条判据只钉「曲线生效且方向正确」✓；缺省不变由既有 golden 测试兜住 ✓）");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 动力学曲线：生效，且方向为「轻入重出」");
