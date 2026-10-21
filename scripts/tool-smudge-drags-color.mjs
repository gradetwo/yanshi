#!/usr/bin/env node
// **`brush_params.smudge` 必须真的**拖带下面的颜色****（第 1585 轮）
//
// **⇒ 为什么写这条**：外部要"融合"（笔带起下面的颜色），而我上一条建议（`dabs_per_basic_radius`）
//   **测不出效果**（第 1584 轮自我纠正）⇒ 而 **`smudge` 这条我能测** ✓
// **⇒ ⇒ 实测**（红底 40px + 蓝笔 14px 横划）：
//   `smudge` 缺省 ⇒ 蓝笔内 **R=40.0**；`smudge: 0.0` ⇒ **R=40.0**（**与缺省相同**）；
//   `smudge: 1.0` ⇒ **R=89.3**（**+49 ⇒ 拖带了红**）✓
//
// **⇒ 断三条**：
//   ① `smudge: 0.0` 与缺省**同色**（**缺省不偷偷开 smudge**）✓
//   ② `smudge: 1.0` 让蓝笔内的 R **显著高于** `0.0` 的（**拖带发生**）✓
//   ③ `smudge: 1.0` 不许把蓝**变成红**（**B 仍明显高于 R** ⇒ 它仍是蓝笔）✓
//
// **⇒ 变异**：把 `smudge` 写死成 0 ⇒ ② 红 ✓
//
// 用法：node scripts/tool-smudge-drags-color.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-smudge-drags-color.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args),
  });
  return reply.json();
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// **坐标按画布缩放**（第 1578 轮的同一个坑）
const canvas = await call("describe_canvas", {});
const W = canvas?.width ?? 0;
const H = canvas?.height ?? 0;
if (!(W > 0) || !(H > 0)) {
  console.error(`  ⊘ 不可判：拿不到画布尺寸`);
  process.exit(3);
}
const cx = W / 2;
const cy = H / 2;
const wide = Math.max(20, Math.round(H / 3));
const thin = Math.max(6, Math.round(H / 8));
const armX = Math.round(W * 0.35);
const armY = Math.round(H * 0.22);

const run = async (tag, smudge) => {
  const layer = `L_${tag}`;
  await call("create_layer", { layer_id: layer, name: layer });
  await call("brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", preview: false,
    points: [[cx - armX, cy, 1], [cx + armX, cy, 1]], size: wide, color: [200, 40, 40],
  });
  const args = {
    layer_id: layer, brush: "100%_Opaque", preview: false,
    points: [[cx - armX * 0.6, cy - armY, 1], [cx + armX * 0.6, cy - armY, 1]],
    size: thin, color: [40, 40, 200],
  };
  if (smudge !== null) args.brush_params = { smudge };
  const reply = await call("brush_stroke", args);
  if (reply?.ok !== true) return null;
  const sampled = await call("sample_colors", {
    points: [[cx, cy - armY], [cx - armX * 0.4, cy - armY], [cx + armX * 0.4, cy - armY]],
  });
  const px = (sampled?.samples || []).map((z) => (z?.rgba || [0, 0, 0, 0]).slice(0, 3));
  if (px.length !== 3) return null;
  return {
    r: px.reduce((a, p) => a + p[0], 0) / px.length,
    b: px.reduce((a, p) => a + p[2], 0) / px.length,
  };
};

const dflt = await run("dflt", null);
const zero = await run("zero", 0.0);
const one = await run("one", 1.0);
if (!dflt || !zero || !one) {
  console.error("  ⊘ 不可判：笔画或采样失败");
  process.exit(3);
}
console.log(`  ⊙ 缺省 R=${dflt.r.toFixed(1)}｜smudge 0.0 R=${zero.r.toFixed(1)}｜smudge 1.0 R=${one.r.toFixed(1)}（B=${one.b.toFixed(1)}）`);

// ① 缺省 == smudge 0
if (Math.abs(dflt.r - zero.r) <= 2) ok(`① 缺省与 \`smudge: 0.0\` **同色**（R ${dflt.r.toFixed(1)} ≈ ${zero.r.toFixed(1)}）⇒ 缺省没偷偷开 ✓`);
else bad(`① 缺省 R=${dflt.r.toFixed(1)} 与 \`smudge: 0.0\` R=${zero.r.toFixed(1)} 差 ${Math.abs(dflt.r - zero.r).toFixed(1)} ⇒ 缺省不是 0`);

// ② smudge 1.0 拖带更多红
if (one.r > zero.r + 10) ok(`② \`smudge: 1.0\` 让蓝笔内 R **升高 ${(one.r - zero.r).toFixed(1)}** ⇒ **拖带了下面的红** ✓`);
else bad(`② \`smudge: 1.0\` 只让 R 升高 ${(one.r - zero.r).toFixed(1)} ⇒ **拖带不明显**`);

// ③ 它仍是蓝笔，不该变成红
if (one.b > one.r + 20) ok(`③ 它仍是**蓝笔**（B=${one.b.toFixed(1)} 明显高于 R=${one.r.toFixed(1)}）✓`);
else bad(`③ B=${one.b.toFixed(1)} 与 R=${one.r.toFixed(1)} 太接近 ⇒ 混成了别的颜色`);

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（smudge 拖带）`); process.exit(1); }
console.log("\n  ✓ `smudge` 真的把下面的颜色带起来了");
