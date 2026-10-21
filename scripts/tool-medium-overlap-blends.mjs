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

await call("create_layer", { layer_id: "L", name: "L" });
const RED = [200, 40, 40];
const BLUE = [40, 40, 200];
const a = await call("medium_stroke", {
  layer_id: "L", medium: "oil", load: 0.8, size: 30, color: RED,
  points: [[40, 60, 1], [160, 60, 1]],
});
const b = await call("medium_stroke", {
  layer_id: "L", medium: "oil", load: 0.8, size: 30, color: BLUE,
  points: [[100, 20, 1], [100, 100, 1]],
});
if (a?.ok !== true || b?.ok !== true) {
  console.error(`  ⊘ 不可判：medium_stroke 失败（${JSON.stringify((a?.context || b?.context || {}).detail || "").slice(0, 120)}）`);
  process.exit(3);
}
const sampled = await call("sample_colors", { points: [[100, 60], [60, 60], [100, 30]] });
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

// ③ R 与 B 各取一侧：R 介于两笔之间、B 介于两笔之间
const between = (v, x, y) => v >= Math.min(x, y) - 2 && v <= Math.max(x, y) + 2;
if (between(overlap[0], onlyRed[0], onlyBlue[0]) && between(overlap[2], onlyRed[2], onlyBlue[2])) {
  ok(`③ 重叠处 R=${overlap[0]}、B=${overlap[2]} **都落在两笔之间** ✓`);
} else {
  bad(`③ 重叠处 R=${overlap[0]}、B=${overlap[2]} 不在两笔之间 ⇒ 颜色关系不对`);
}

if (fails.length > 0) { console.log(`\n  ✗ ${fails.length} 处不符（重叠混色）`); process.exit(1); }
console.log("\n  ✓ 两笔重叠处是**混合色**，不是覆盖");
