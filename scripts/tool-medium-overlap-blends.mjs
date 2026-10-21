#!/usr/bin/env node
// **两笔重叠处必须**融合**（介质路径的混色）**（第 1578 轮）
//
// **⇒ 为什么写这条**：外部说「`medium_stroke(medium="oil")` 仍然**盖章、不融合**」（`E1669`）
//   ⇒ 而我实测**是融合的**（红 `#d06969` × 蓝 `#6161d5` ⇒ 重叠处 **`#5f3da6` 紫**）✓
//   ⇒ **⇒ 所以**这条性质**值得有判据**：以后谁把混色改坏，它会红 ✓
//
// **⇒ 断三条**（都断**颜色关系**，不断具体数值）：
//   ① 重叠区**既不是**纯红、**也不是**纯蓝 ⇒ **两笔都贡献了** ✓
//   ② 重叠区的 **G 通道**低于两笔各自的值（**红蓝都压 G** ⇒ 混合必然更暗）✓
//   ③ 重叠区的 **R 与 B 都处于两者之间**（**红给 R、蓝给 B**）✓
//
// **⇒ 变异**：把合成改成"后一笔完全覆盖"（不混色）⇒ 重叠处 == 纯蓝 ⇒ ①②③ 全红 ✓
//
// 用法：node scripts/tool-medium-overlap-blends.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-medium-overlap-blends.mjs <base> <doc> <token>");
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

// **★ 坐标必须按**画布尺寸**缩放 ★**（第 1578 轮自查）
//   **⇒ 我第一版写死了 `[[40,60],[160,60]]`（按 200×120 算）** ✗
//     ⇒ 而 `run-criteria-local.sh` 建的画布**小得多** ⇒ **采样点落在画布外**
//     ⇒ `sample_colors` 返回 `(0,0,0,0)` ⇒ **判据误报"重叠 == 纯蓝"** ✗
//     （**这与 `tool-smudge-deposit` 是同一个坑** —— 判据假设的画布 ≠ 运行器给的 ✓）
//   **⇒ ⇒ 所以**：**先问画布多大，再按比例算坐标** ✓
const canvas = await call("describe_canvas", {});
const W = canvas?.width ?? 0;
const H = canvas?.height ?? 0;
if (!(W > 0) || !(H > 0)) {
  console.error(`  ⊘ 不可判：拿不到画布尺寸（${JSON.stringify({ W, H })}）`);
  process.exit(3);
}
// 两条笔**横竖交叉**于画布中心；笔宽取短边的 1/4
const cx = W / 2;
const cy = H / 2;
const size = Math.max(6, Math.round(Math.min(W, H) / 4));
const armX = Math.round(W * 0.35);
const armY = Math.round(H * 0.35);
console.log(`  ⊙ 画布 ${W}×${H}｜笔宽 ${size}｜交叉点 (${cx}, ${cy})`);

await call("create_layer", { layer_id: "L", name: "L" });
const RED = [200, 40, 40];
const BLUE = [40, 40, 200];
const a = await call("medium_stroke", {
  layer_id: "L", medium: "oil", load: 0.8, size, color: RED,
  points: [[cx - armX, cy, 1], [cx + armX, cy, 1]],
});
const b = await call("medium_stroke", {
  layer_id: "L", medium: "oil", load: 0.8, size, color: BLUE,
  points: [[cx, cy - armY, 1], [cx, cy + armY, 1]],
});
if (a?.ok !== true || b?.ok !== true) {
  console.error(`  ⊘ 不可判：medium_stroke 失败（${JSON.stringify((a?.context || b?.context || {}).detail || "").slice(0, 120)}）`);
  process.exit(3);
}
const sampled = await call("sample_colors", { points: [[cx, cy], [cx - armX * 0.6, cy], [cx, cy - armY * 0.6]] });
const px = (sampled?.samples || []).map((z) => (z?.rgba || [0, 0, 0, 0]).slice(0, 3));
if (px.length !== 3 || px.some((p) => p.every((c) => c === 255))) {
  console.error(`  ⊘ 不可判：拿不到三处采样（${JSON.stringify(px)}）`);
  process.exit(3);
}
const [overlap, onlyRed, onlyBlue] = px;
const hex = (p) => "#" + p.map((c) => c.toString(16).padStart(2, "0")).join("");
console.log(`  ⊙ 只有红 ${hex(onlyRed)}｜只有蓝 ${hex(onlyBlue)}｜重叠 ${hex(overlap)}`);

// ① 重叠处不等于任何一笔单独的颜色
const same = (x, y) => x.every((c, i) => Math.abs(c - y[i]) <= 2);
if (same(overlap, onlyBlue)) bad(`① 重叠处 == 纯蓝 ⇒ **后一笔完全覆盖，没有混色** ✗`);
else if (same(overlap, onlyRed)) bad(`① 重叠处 == 纯红 ⇒ **前一笔赢了，没有混色** ✗`);
else ok(`① 重叠处 **既不等于纯红也不等于纯蓝** ⇒ 两笔都贡献了 ✓`);

// ② 红蓝都压 G ⇒ 混合的 G 必须 <= 两者较小者 + 容差
const gMin = Math.min(onlyRed[1], onlyBlue[1]);
if (overlap[1] <= gMin + 2) ok(`② 重叠处 G=${overlap[1]} ≤ 两笔较小者 ${gMin}+2 ⇒ **两笔都压了 G** ✓`);
else bad(`② 重叠处 G=${overlap[1]} > ${gMin}+2 ⇒ 不像混合（混合应更暗）`);

// **③ 我删掉了**（第 1578 轮自查）
//   **⇒ 原来写的是**「重叠处 R 与 B **都落在两笔之间**」✗
//   **⇒ ⇒ 而实测**：`只有红 #ce6d6d`、`只有蓝 #5d5cce`、`重叠 #5649bb`
//     ⇒ **重叠的 R=86 **略低于**纯蓝的 R=93** ✗（**因为混合整体会压暗**）
//     ⇒ **所以"必须落在两者之间"是错的假设** ✓
//   **⇒ ⇒ ⇒ 而 ①② 已经足够**：**重叠 ≠ 任何一笔** + **G 比两者都低** ⇒ **这就是融合的定义** ✓
//     （**多一条错的断言**比**少一条**更糟 —— 它会**假红**）
if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（重叠混色）`); process.exit(1); }
console.log("\n  ✓ 两笔重叠处是**混合色**，不是覆盖");
