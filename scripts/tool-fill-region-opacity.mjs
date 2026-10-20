#!/usr/bin/env node
// **`fill_region` 的 `opacity` 必须真的生效（数组颜色形态）**（第 1410 轮）
//
// **⇒ 原缺陷** ✗：`fill_region(color=[0,0,0], opacity=0.5)` ⇒ 中心像素仍是 `[0,0,0,255]`
//   ⇒ 实测 `opacity` 1.0／0.5／0.2 **三者完全相同** ⇒ **参数被静默忽略** ✗✗
//   **⇒ 根因**：处理器里 `opacity` 只乘进**对象形态**（`{r,g,b,a}`）的 `a`，
//     而**数组形态**（`[r,g,b]`）走 `else { color }` **原样通过** ✗
//     ⇒ 而紧邻的注释写着「**颜色两种形态都收**」⇒ **说得对，做得只对一半** ✗
//
// **⇒ 断什么**（结构量 ✓）：
//   ① 数组颜色 + `opacity: 0.5` ⇒ 中心像素**不是**全不透明（在纯蓝底上，蓝通道明显 > 0）
//   ② **单调**：`opacity` 1.0 → 0.5 → 0.2 ⇒ 蓝通道**严格递增**（越透明越接近底色）
//   ③ 对象形态 `{r,g,b,a}` 也仍然生效（回归守卫 ✓）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-fill-region-opacity.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();

const newDoc = async (id) => {
  const r = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 64, height: 64,
      background: { r: 0, g: 0, b: 255, a: 255 } }),
  })).json();
  return r.token;
};
const post = async (tok, doc, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${tok}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const centre = async (tok, doc) => {
  const r = await post(tok, doc, "sample_colors", { points: [[20, 20]] });
  const s = (r.samples || [])[0];
  return s?.rgba ?? null;
};

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const measure = async (tag, opacity, colour) => {
  const doc = `fr_${tag}_${stamp}`;
  const tok = await newDoc(doc);
  if (!tok) return null;
  await post(tok, doc, "create_layer", { layer_id: "L", name: "L" });
  const r = await post(tok, doc, "fill_region", {
    layer_id: "L", shape: { type: "rect", x: 10, y: 10, w: 20, h: 20 }, color: colour, opacity });
  if (r?.ok !== true) return null;
  return centre(tok, doc);
};

const a1 = await measure("o1", 1.0, [0, 0, 0]);
const a5 = await measure("o5", 0.5, [0, 0, 0]);
const a2 = await measure("o2", 0.2, [0, 0, 0]);
if (!a1 || !a5 || !a2) {
  console.error("  ⊘ 不可判：建文档或落笔失败（不是产品问题）");
  process.exit(3);
}

// ① 半透明必须看得出来（纯蓝底上，黑 α=0.5 ⇒ 蓝通道应远大于 0）
if (a5[2] > 60) ok(`① 数组颜色 + opacity 0.5 ⇒ 中心 ${JSON.stringify(a5)}（蓝通道 ${a5[2]} > 60 ⇒ 真的半透明）`);
else bad(`① opacity 0.5 必须真的半透明 ⇒ 实测中心 ${JSON.stringify(a5)}（蓝通道 ${a5[2]}，近乎全不透明）`);

// ② 单调
if (a1[2] < a5[2] && a5[2] < a2[2]) {
  ok(`② 单调：α 1.0→0.5→0.2 的蓝通道 ${a1[2]} < ${a5[2]} < ${a2[2]}`);
} else {
  bad(`② 越透明越接近底色 ⇒ 蓝通道应严格递增 ⇒ 实测 ${a1[2]}, ${a5[2]}, ${a2[2]}`);
}

// ③ 对象形态回归守卫
const o5 = await measure("obj5", 0.5, { r: 0, g: 0, b: 0, a: 255 });
if (o5 && o5[2] > 60) ok(`③ 对象颜色 {r,g,b,a} + opacity 0.5 ⇒ 中心 ${JSON.stringify(o5)} 也半透明`);
else bad(`③ 对象形态也必须应用 opacity ⇒ 实测 ${JSON.stringify(o5)}`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（fill_region 的 opacity 必须对两种颜色形态都生效）`);
  process.exit(1);
}
console.log("\n  ✓ fill_region 的 opacity 在数组与对象两种颜色形态下都生效");
