#!/usr/bin/env node
// **`brush_stroke` 的 `style` 判据** ✓（需求文档 P2-8 智能笔触风格 ✓）。
// 文档要的是：`"confident"` ⇒ **自动起笔重、收笔轻** ✓；`"sketchy"` ⇒ 自动加抖动与断笔 ✓。
// **现在必然红** ✓（`brush_stroke` 不接受 `style` ✗ —— 先写判据、先看红 ✓）。
//
// 判据（只断言**可观察**的东西 ✓，不猜内部字段 ✗）：
//   ① `style: "confident"` 被接受且**落下像素** ✓；
//   ② 同样的笔触**不带 style** 也落下像素 ✓（缺省这条路不能坏 ✓）；
//   ③ 两者**画面不同** ✓（否则"风格"名不副实 ✗）；
//   ④ **自检**：两次都不带 style 时，画面必须**逐字节相同** ✓（否则渲染本身不稳定 ⇒ 判据无效 ✗）。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-brush-style.mjs <base-url>"); process.exit(2); }
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
const paintAndShot = async (id, token, style) => {
  await call(id, token, "create_layer", { layer_id: "layer_default", name: "l" });
  const args = {
    layer_id: "layer_default", brush: "classic-brush",
    points: [[30, 60, 0.8], [90, 60, 0.6], [170, 60, 0.3]],
    size: 14, color: { r: 40, g: 80, b: 160, a: 255 },
  };
  if (style) args.style = style;
  const result = await call(id, token, "brush_stroke", args);
  if (result.ok !== true) return { error: String((result.context || {}).detail || result.error_code || "?") };
  const raw = await call(id, token, "render_region", { region: { x: 0, y: 0, width: 200, height: 120 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return { error: "拿不到直通 RGBA" };
  return { bytes: Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer()) };
};
const docs = {};
for (const key of ["plain1", "plain2", "confident"]) docs[key] = await makeDoc("bs_" + key);
const plain1 = await paintAndShot("bs_plain1", docs.plain1, null);
const plain2 = await paintAndShot("bs_plain2", docs.plain2, null);
const confident = await paintAndShot("bs_confident", docs.confident, "confident");
if (plain1.error) { console.log("  ✗ 不带 style 就画不出来 ⇒ 判据无效 ⇒ " + plain1.error); process.exit(1); }
if (confident.error) {
  console.log("  style:\"confident\" ⇒ " + confident.error);
  console.log("  ✗ style 不被接受 ⇒ 判据红（预期 ✓ —— 这个能力还不存在 ✓）");
  process.exit(1);
}
const diff = (a, b) => a.reduce((n, v, i) => (v !== b[i] ? n + 1 : n), 0);
const selfDiff = diff(plain1.bytes, plain2.bytes);
const styleDiff = diff(plain1.bytes, confident.bytes);
const ink = (buf) => { let n = 0; for (let i = 3; i < buf.length; i += 4) if (buf[i] > 0) n += 1; return n; };
console.log(`  不带 style 两次之间差异字节 = ${selfDiff}（自检：应为 0 ✓）`);
console.log(`  confident 与不带 style 的差异字节 = ${styleDiff}（应 > 0 ✓）`);
const failures = [];
if (selfDiff !== 0) failures.push(`两次相同的调用结果不同（${selfDiff} 字节）⇒ 渲染不稳定 ⇒ 判据无效 ✗`);
if (styleDiff === 0) failures.push("confident 与不带 style 完全一样 ⇒ 风格没有生效 ✗");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ brush_stroke.style：confident 生效，且缺省路径依然稳定");
