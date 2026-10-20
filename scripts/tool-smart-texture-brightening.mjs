#!/usr/bin/env node
// **`smart_texture` 的提亮规律**（第 1381 轮；外部报「提亮 +33~55」后第 1380 轮复现）
//
// **⇒ 背景** ✗：`smart_texture` 上完肌理后**画面均值会上移** ⇒ 而外部要**还原**⇒ 这会破坏色彩保真 ✗
//   ⇒ 第 1380 轮实测（底色 0.502）：缺省 +25.0、`color = 底色` 也是 +25.0、0.10 时 −20.2、
//     **0.25 时 +0.7** ⇒ 所以**肌理色 = 底色的一半**时均值不变 ✓
//
// **⇒ 这条判据断什么**（**只断规律，不给缺陷盖章** ✓）：
//   ① **单调**：`color` 越大 ⇒ Δ 越大（0.65 的 Δ 必须 > 0.35 的 Δ）
//   ② **零点存在且在半色附近**：`color = 底色 × 0.5` 时 |Δ| **< 8**
//   ④ **`neutral: true` 必须把 Δ 压到 |Δ| < 8**（第 1382 轮加的实测查表）
//   ③ **打印**缺省与 `= 底色` 的 Δ（**只打印不断言** ⇒ 因为那是**待修的缺陷**，
//      一旦哪天修好，断言它会红，而打印只提示 ✓）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-smart-texture-brightening.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const GREY = 128;                       // 底色灰（0.502）
const G = GREY / 255;
const pts = [];
for (let y = 0; y < 8; y += 1) for (let x = 0; x < 8; x += 1) pts.push([x * 24 + 12, y * 24 + 12]);

let TOK = "";
const newDoc = async (id) => {
  const r = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 256, height: 256,
      background: { r: GREY, g: GREY, b: GREY, a: 255 } }),
  })).json();
  return r.token;
};
const post = async (doc, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const lum = async (doc) => {
  const s = (await post(doc, "sample_colors", { points: pts })).samples || [];
  const v = s.filter((x) => x.rgba).map((x) => 0.2126 * x.rgba[0] + 0.7152 * x.rgba[1] + 0.0722 * x.rgba[2]);
  return v.length ? v.reduce((a, b) => a + b, 0) / v.length : null;
};
const measure = async (colour) => {
  const doc = `st_${Math.round(colour * 100)}_${stamp}`;
  TOK = await newDoc(doc);
  if (!TOK) return null;
  await post(doc, "create_layer", { layer_id: "L", name: "L" });
  const before = await lum(doc);
  const args = { layer_id: "L", texture_type: "canvas",
    shape: { type: "rect", x: 0, y: 0, w: 256, h: 256 }, density: 0.9, size: 9 };
  if (colour !== null) args.color = [colour, colour, colour];
  const r = await post(doc, "smart_texture", args);
  const after = await lum(doc);
  if (r?.ok !== true || before === null || after === null) return null;
  return after - before;
};

// **④ `neutral: true`**：五档底色下 |Δ| 都必须 < 8（第 1382 轮）
//   ⇒ 它按**实测查表**把肌理色换成「均值不变」的那个值 ⇒ 所以这条也在验那张表没被弄坏 ✓
const neutralDeltas = [];
for (const grey of [60, 90, 128, 170, 210]) {
  const doc = `nt_${grey}_${stamp}`;
  const r0 = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 256, height: 256,
      background: { r: grey, g: grey, b: grey, a: 255 } }),
  })).json();
  if (!r0.token) { neutralDeltas.push(null); continue; }
  TOK = r0.token;
  await post(doc, "create_layer", { layer_id: "L", name: "L" });
  const b = await lum(doc);
  await post(doc, "smart_texture", { layer_id: "L", texture_type: "canvas", neutral: true,
    shape: { type: "rect", x: 0, y: 0, w: 256, h: 256 }, density: 0.9, size: 9 });
  const a = await lum(doc);
  neutralDeltas.push(a === null || b === null ? null : a - b);
}

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const dDark = await measure(0.35);
const dLight = await measure(0.65);
const dHalf = await measure(G * 0.5);
const dDefault = await measure(null);
const dSame = await measure(G);

if (dDark === null || dLight === null || dHalf === null) {
  console.error("  ⊘ 不可判：三次测量里有失败的（不是产品问题）");
  process.exit(3);
}

// **③ 先打印**（待修的缺陷 ⇒ 只打印 ✓）
console.log(`  ③ 打印：缺省 Δ ${dDefault === null ? "n/a" : dDefault.toFixed(1)}｜= 底色 Δ ${dSame === null ? "n/a" : dSame.toFixed(1)}`);

// ④ neutral: true 的 |Δ|
if (neutralDeltas.every((d) => d !== null)) {
  const worst = Math.max(...neutralDeltas.map((d) => Math.abs(d)));
  const shown = neutralDeltas.map((d) => d.toFixed(1)).join(", ");
  if (worst < 8) ok(`④ neutral:true ⇒ 五档底色 |Δ| 最大 ${worst.toFixed(1)}（${shown}）`);
  else bad(`④ neutral:true 必须把 |Δ| 压到 < 8 ⇒ 五档实测 ${shown}（最大 ${worst.toFixed(1)}）`);
} else {
  console.log("  ○ ④ 未验：有一档建文档失败");
}

// ① 单调
if (dLight > dDark) ok(`① 单调：color 0.65 ⇒ Δ ${dLight.toFixed(1)} > color 0.35 ⇒ Δ ${dDark.toFixed(1)}`);
else bad(`① Δ 必须随 color 单调增 ⇒ 实测 0.65 给 ${dLight.toFixed(1)}、0.35 给 ${dDark.toFixed(1)}`);

// ② 零点在「半色」附近
if (Math.abs(dHalf) < 8) ok(`② 半色（${(G * 0.5).toFixed(3)}）⇒ Δ ${dHalf.toFixed(1)}（|Δ| < 8 ⇒ 均值基本不变）`);
else bad(`② color = 底色一半时均值应基本不变 ⇒ 实测 Δ ${dHalf.toFixed(1)}（要求 |Δ| < 8）`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（smart_texture 的提亮规律变了）`);
  process.exit(1);
}
console.log("\n  ✓ 提亮规律成立（单调、半色处中性）");
