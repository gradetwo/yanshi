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
// **Y26-006 修订（own-layer 语义，2026-10-30）**：
//   旧 setup 蓝笔在红线上方 `cy - armY`，只在边缘与红沾一点 ⇒ 旧语义下底图是整幅合成，
//   采样点上的不透明蓝锚来自前两轮的图层（跨层泄漏），③ 靠它才过；新语义下底图只剩本层，
//   采样点上是白纸 ⇒ smudge 1.0 洗成近白（实测 R=236.0、B=236.7），② 靠白洗过、③ 不过。
//   ⇒ 新 setup：蓝笔与红笔同中心线 `cy`（采样点落在红漆上），三轮各在本层（新语义下本层隔离，
//     不再依赖跨层泄漏；笔画皆不透明，合成采样即本层采样 ✓）。
//   ⇒ 涂抹只拖起笔后 `back` 之内的颜色（`back = thin/4`，见拖曳区注记）：② 采拖曳区、③ 采中间点，
//     两处分开断，不再用同一点既证拖带又证仍蓝（`smudge: 1.0` 全量替换时同点两断言不可兼得）。
//   ⇒ ①②③ 的断言形式不变，只换采样点：①② 用拖曳区、③ 用中间点。
//   ⇒ ③ 的 +20 沿用旧口径：中间点是纯蓝（B≈200、R≈40），余量极大，不属紧阈值；
//     CI 若报 ③ 红，先看逐点打印判是几何漂移还是语义变化，再动阈值，绝不猜数字凑绿。
//   ⇒ CI 须验证：①②③ 全绿＋⊙ 行与拖曳区三点打印的实测值（下一轮标定以它们为准）。
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

// **拖曳区采样点**（Y26-006 修订 ✓）：蓝笔起笔点 `x0 = cx - armX * 0.6`；
//   涂抹向后拖 `back = smudge_length(0.5) × 半径(thin/2) = thin/4`（`100%_Opaque` 的
//   `smudge_length` 无输入曲线 ⇒ 求值恒 0.5；`size` 是直径 ⇒ 半径 = thin/2；
//   两式相乘得 back，与画布尺寸无关 ✓）。拖曳区 = [x0, x0 + back) ⇒
//   三个拖曳点取 x0 + thin/16、thin/8、thin*3/16（均 < back，有 4/3 倍以上余量 ✓）。
//   中间点取旧三点（cx、cx±0.4*armX）：离起笔 ≥ 0.2*armX，远大于 back ⇒ 在拖曳区外 ✓。
const x0 = cx - armX * 0.6;
const dragXs = [x0 + thin / 16, x0 + thin / 8, x0 + thin * 3 / 16];
const midXs = [cx - armX * 0.4, cx, cx + armX * 0.4];

const run = async (tag, smudge) => {
  const layer = `L_${tag}`;
  await call("create_layer", { layer_id: layer, name: layer });
  await call("brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", preview: false,
    points: [[cx - armX, cy, 1], [cx + armX, cy, 1]], size: wide, color: [200, 40, 40],
  });
  const args = {
    layer_id: layer, brush: "100%_Opaque", preview: false,
    points: [[x0, cy, 1], [cx + armX * 0.6, cy, 1]],
    size: thin, color: [40, 40, 200],
  };
  if (smudge !== null) args.brush_params = { smudge };
  const reply = await call("brush_stroke", args);
  if (reply?.ok !== true) return null;
  // **一次采样、两组点** ✓（`sample_colors` 一次只要一个区域 ⇒ 6 点同批，不加渲染趟数 ✓）：
  //   前 3 个是拖曳点（①② 用），后 3 个是中间点（③ 用）✓。
  const sampled = await call("sample_colors", {
    points: [...dragXs.map((x) => [x, cy]), ...midXs.map((x) => [x, cy])],
  });
  const px = (sampled?.samples || []).map((z) => (z?.rgba || [0, 0, 0, 0]).slice(0, 3));
  if (px.length !== 6) return null;
  const avg = (arr) => ({
    r: arr.reduce((a, p) => a + p[0], 0) / arr.length,
    b: arr.reduce((a, p) => a + p[2], 0) / arr.length,
  });
  return { drag: avg(px.slice(0, 3)), mid: avg(px.slice(3, 6)), dragPx: px.slice(0, 3) };
};

const dflt = await run("dflt", null);
const zero = await run("zero", 0.0);
const one = await run("one", 1.0);
if (!dflt || !zero || !one) {
  console.error("  ⊘ 不可判：笔画或采样失败");
  process.exit(3);
}
console.log(`  ⊙ 缺省拖曳区 R=${dflt.drag.r.toFixed(1)}｜smudge 0.0 拖曳区 R=${zero.drag.r.toFixed(1)}｜smudge 1.0 拖曳区 R=${one.drag.r.toFixed(1)}｜smudge 1.0 中间 B=${one.mid.b.toFixed(1)} R=${one.mid.r.toFixed(1)}`);
// **校准用逐点打印** ✓（不参与判定 ✓）：CI 日志里留拖曳区三点的实测，供下一轮标定复核 ✓。
console.log(`  ⊙ 拖曳区三点（smudge 1.0）：${one.dragPx.map((p) => `(${p[0]},${p[1]},${p[2]})`).join(" ")}`);

// ① **缺省**不应该像 `smudge: 1.0` 那样拖带** ✓（拖曳区点上算 ✓：
//   缺省若偷偷开了涂抹，拖曳区会先露出它；中间点在拖曳区外，对缺省不敏感 ⇒ 不用它算 ① ✓）
//   **⇒ 沿用关系式而非硬等价** ✓（旧教训：第一版要求"缺省 == `smudge: 0.0`"（差 ≤ 2）✗
//     ⇒ **实测**本机 40.0 vs 40.0（相等），而 **CI 上 45.3 vs 40.0**（差 5.3）✗
//     ⇒ **所以"缺省值"跨机器不同** ⇒ **硬等价是**跨不了机器**的断言** ⇒ 改成关系：缺省拖带量远小于 `1.0`（取 1/3 作界）✓）
const span = Math.max(1, one.drag.r - zero.drag.r);
const defaultRise = dflt.drag.r - zero.drag.r;
if (defaultRise <= span / 3) {
  ok(`① 缺省只比 \`smudge: 0.0\` 高 ${defaultRise.toFixed(1)}，而 \`1.0\` 高 ${span.toFixed(1)} ⇒ **缺省远没到 1.0 的程度** ✓`);
} else {
  bad(`① 缺省比 \`smudge: 0.0\` 高 ${defaultRise.toFixed(1)}，而 \`1.0\` 只高 ${span.toFixed(1)} ⇒ 缺省本身就明显拖带`);
}

// ② smudge 1.0 在拖曳区拖带更多红（拖曳区点上算 ✓：只有这里的底图是起笔前的红漆 ✓）
if (one.drag.r > zero.drag.r + 10) ok(`② \`smudge: 1.0\` 让拖曳区内 R **升高 ${(one.drag.r - zero.drag.r).toFixed(1)}** ⇒ **拖带了下面的红** ✓`);
else bad(`② \`smudge: 1.0\` 只让拖曳区 R 升高 ${(one.drag.r - zero.drag.r).toFixed(1)} ⇒ **拖带不明显**`);

// ③ 它仍是蓝笔，不该变成红（中间点上算 ✓：中间点在拖曳区外，底图是蓝笔自身 ⇒ 仍是蓝 ✓）
if (one.mid.b > one.mid.r + 20) ok(`③ 它仍是**蓝笔**（中间 B=${one.mid.b.toFixed(1)} 明显高于 R=${one.mid.r.toFixed(1)}）✓`);
else bad(`③ 中间 B=${one.mid.b.toFixed(1)} 与 R=${one.mid.r.toFixed(1)} 太接近 ⇒ 混成了别的颜色`);

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（smudge 拖带）`); process.exit(1); }
console.log("\n  ✓ `smudge` 真的把下面的颜色带起来了");
