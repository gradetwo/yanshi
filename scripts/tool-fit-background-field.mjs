#!/usr/bin/env node
// **`fit_background_field`**：掩膜外的像素拟合成平滑场、铺进掩膜内（第 1447 轮；外部功能请求）
//
// **⇒ 它解决什么** ✓：外部在做「只画背景」⇒ 他要「一个工具直接把掩膜外像素拟合成平滑背景场并铺上」
//   ⇒ 而这个需求在本仓**也出现过**：`smart_sample` 用 8×8 的场近似参考图 ⇒ 他刚诊断出
//   「插值点太少 ⇒ 必然斑块/竖纹」（`E1408`）⇒ **多项式场没有格点 ⇒ 结构上不可能有折痕** ✓
//
// **⇒ 断什么**（**结构量** ✓）：
//   ① 掩膜内的一整块**白色**必须**被替换**（不能残留 255）⇒ 证明"真的铺进去了" ✓
//   ② 填充结果**平滑**：沿一条线逐点灰度，**相邻差**的最大值必须**小**（阶梯底 ⇒ 拟合值连续）✓
//   ③ **残差被报出来**（`residual_rms` 是三个数）⇒ 调用方能判断 degree 够不够 ✓
//   ④ 反证：掩膜外的像素**不许**被改动（取掩膜外一点，前后灰度相同）✓
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-fit-background-field.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const doc = `bgf_${stamp}`;
const made = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 300,
    background: { r: 255, g: 255, b: 255, a: 255 } }),
})).json();
if (!made.token) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${made.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
await post("create_layer", { layer_id: "L", name: "L" });
// 二次渐变的底（12 条横带）
for (let k = 0; k < 12; k += 1) {
  const y = 10 + k * 23;
  const g = Math.round(255 * ((y / 300 - 0.5) ** 2 * 4));
  await post("fill_region", { layer_id: "L", shape: { type: "rect", x: 10, y, w: 380, h: 23 },
    color: [g, g, g], opacity: 1.0 });
}
// 掩膜区涂白（要被填掉）
await post("fill_region", { layer_id: "L", shape: { type: "rect", x: 120, y: 80, w: 160, h: 140 },
  color: [255, 255, 255], opacity: 1.0 });
const outsideBefore = (await post("sample_colors", { points: [[40, 150]] })).samples?.[0]?.rgba;

const r = await post("fit_background_field", {
  layer_id: "L", mask: { type: "rect", x: 120, y: 80, w: 160, h: 140 }, degree: 2, sample_stride: 2 });
// **⇒ 工具在**合法输入**下报错 = **产品失败**（红），不是"不可判"** ✗
//   **⇒ 为什么**：我第一版在这里 `exit(3)` ⇒ 而变异（不填像素 ⇒ 工具报"掩膜内没有一个像素"）
//     ⇒ 判据只报"不可判" ⇒ **变异证明不了任何东西** ✗（当场被变异抓住 ✓）
if (r?.ok !== true) {
  console.error(`  ✗ fit_background_field 在合法输入上失败 ⇒ ${String((r?.context || {}).detail || "").slice(0, 120)}`);
  process.exit(1);
}

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① 掩膜内的白色被替换
const pts = [];
for (let y = 85; y <= 215; y += 10) pts.push([200, y]);
const inside = (await post("sample_colors", { points: pts })).samples?.map((s) => s.rgba?.[0]) ?? [];
if (inside.length > 0 && !inside.some((v) => v === 255)) {
  ok(`① 掩膜内 ${inside.length} 个采样点**没有**残留白色 ⇒ 真的铺进去了（灰度 ${inside.slice(0, 4).join(",")}…）`);
} else {
  bad(`① 掩膜内不应残留 255 ⇒ 实测 ${JSON.stringify(inside)}`);
}

// ② 平滑（相邻差小）：阶梯底拟合出来必须是连续的
if (inside.length > 2) {
  let maxStep = 0;
  for (let i = 1; i < inside.length; i += 1) maxStep = Math.max(maxStep, Math.abs(inside[i] - inside[i - 1]));
  if (maxStep <= 60) ok(`② 填充平滑：相邻采样点灰度最大差 ${maxStep} ≤ 60（阶梯底被拟合成连续场）`);
  else bad(`② 填充应平滑 ⇒ 相邻最大差 ${maxStep}（> 60 ⇒ 场不连续）`);
} else {
  bad("② 采样点不足，无法判断平滑性");
}

// ③ 残差被报出来
const rms = r.residual_rms;
if (Array.isArray(rms) && rms.length === 3 && rms.every((v) => typeof v === "number")) {
  ok(`③ 残差已报出：[${rms.map((v) => v.toFixed(1)).join(", ")}]（degree 2 对阶梯底 ≈ 40 是合理的）`);
} else {
  bad(`③ 必须返回三通道 residual_rms ⇒ 实测 ${JSON.stringify(rms)}`);
}

// ④ 反证：掩膜外不许被改
const outsideAfter = (await post("sample_colors", { points: [[40, 150]] })).samples?.[0]?.rgba;
if (outsideBefore && outsideAfter && outsideBefore[0] === outsideAfter[0]) {
  ok(`④ 掩膜外像素未被改动（前 ${outsideBefore[0]}、后 ${outsideAfter[0]}）`);
} else {
  bad(`④ 掩膜外不应被改 ⇒ 前 ${JSON.stringify(outsideBefore)}、后 ${JSON.stringify(outsideAfter)}`);
}

// ⑤ **degree 真的在起作用**：1 的残差必须 **>** 3 的残差（同一份数据）
//   **⇒ 为什么这条重要** ✗：前四条只证明"填进去了"，**没证明"拟合在拟合"** ⇒
//     把 `degree` 写死成 2 也能全过 ✗ ⇒ 这条**封住它** ✓
//   **⇒ ⇒ 实测**：degree 1 ⇒ 73.5、2 ⇒ 39.7、3 ⇒ **36.6** ⇒ **单调下降** ✓
//   **⇒ ⇒ ⇒ 而 `degree=9` 必须**夹到 3**（返回里 `degree` 是**生效值** ⇒ 调用方看得见 ✓）
{
  const doc2 = `bgf2_${stamp}`;
  const made2 = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc2, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!made2.token) {
    console.log("  ○ ⑤ 未验（建不出第二个文档）");
  } else {
    const p2 = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${doc2}&token=${made2.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await p2("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 12; k += 1) {
      const y = 10 + k * 23;
      const g = Math.round(255 * ((y / 300 - 0.5) ** 2 * 4));
      await p2("fill_region", { layer_id: "L", shape: { type: "rect", x: 10, y, w: 380, h: 23 },
        color: [g, g, g], opacity: 1.0 });
    }
    const mask = { type: "rect", x: 120, y: 80, w: 160, h: 140 };
    const d1 = await p2("fit_background_field", { layer_id: "L", mask, degree: 1 });
    const d3 = await p2("fit_background_field", { layer_id: "L", mask, degree: 3 });
    const d9 = await p2("fit_background_field", { layer_id: "L", mask, degree: 9 });
    const r1 = d1?.residual_rms?.[0];
    const r3 = d3?.residual_rms?.[0];
    if (typeof r1 === "number" && typeof r3 === "number" && r3 < r1) {
      ok(`⑤ degree 真的在拟合：degree 1 残差 ${r1.toFixed(1)} > degree 3 残差 ${r3.toFixed(1)}`);
    } else {
      bad(`⑤ degree 1 与 3 的残差必须不同（否则 degree 是摆设）⇒ 实测 1:${r1}、3:${r3}`);
    }
    if (d9?.degree === 3) ok(`⑤ degree=9 被**夹到 3**且返回里如实给出（degree=${d9.degree}）`);
    else bad(`⑤ degree=9 必须夹到 3 ⇒ 实测 degree=${d9?.degree}`);
  }
}

// ⑥ **单色场必须**说出来**（第 1451 轮；外部 `§222` 的根因）
//   **⇒ 外部报的** ✗：「对象存在但渲染 100% 空白」⇒ 我复现后发现**对象真的画了、画的是纯白** ⇒
//     **掩膜外的样本本来就是白的** ⇒ 拟合出 255 ⇒ `mean 255 / std 0` ⇒ **看起来像没画** ✗
//   **⇒ ⇒ 所以**判据断两件事**：
//     ① **空白层**（掩膜外全白）⇒ `field_uniform === true` **且** `warnings` 非空 ⇒ **说得出来** ✓
//     ② **渐变层**（掩膜外有明暗）⇒ `field_uniform === false` **且** `warnings` 为空 ⇒ **不许狼来了** ✓
{
  const docW = `bgf_white_${stamp}`;
  const madeW = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docW, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeW.token) {
    console.log("  ○ ⑥ 未验（建不出文档）");
  } else {
    const pW = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docW}&token=${madeW.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pW("create_layer", { layer_id: "BG", name: "BG" });
    const w = await pW("fit_background_field", {
      layer_id: "BG", mask: { type: "rect", x: 120, y: 80, w: 160, h: 140 }, degree: 2 });
    const said = Array.isArray(w?.warnings) && w.warnings.some((x) => /单色/.test(String(x)));
    if (w?.field_uniform === true && said) {
      ok(`⑥ 空白层（掩膜外全白）⇒ 场被判定为单色**并说出来**（range ${JSON.stringify(w.field_range)}）`);
    } else {
      bad(`⑥ 空白层的场是单色 ⇒ 必须 field_uniform=true 且给出 warning ⇒ 实测 ${JSON.stringify(w?.field_range)}、uniform=${w?.field_uniform}`);
    }
    // ② 渐变层不许误报（**自带一个渐变文档** ⇒ 不依赖 ⑤ 块的局部变量 ✓）
    const docG = `bgf_grad_${stamp}`;
    const madeG = await (await fetch(`${base}/api/documents`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: docG, width: 400, height: 300,
        background: { r: 255, g: 255, b: 255, a: 255 } }),
    })).json();
    const pG = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docG}&token=${madeG.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pG("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 12; k += 1) {
      const y = 10 + k * 23;
      const g = Math.round(255 * ((y / 300 - 0.5) ** 2 * 4));
      await pG("fill_region", { layer_id: "L", shape: { type: "rect", x: 10, y, w: 380, h: 23 },
        color: [g, g, g], opacity: 1.0 });
    }
    const g2 = await pG("fit_background_field", {
      layer_id: "L", mask: { type: "rect", x: 120, y: 80, w: 160, h: 140 }, degree: 2 });
    if (g2?.field_uniform === false && (g2?.warnings === null || g2?.warnings === undefined)) {
      ok(`⑥ 渐变层不误报（range ${JSON.stringify(g2.field_range)}、warnings 为空）`);
    } else {
      bad(`⑥ 渐变层不该被判成单色 ⇒ 实测 uniform=${g2?.field_uniform}、warnings=${JSON.stringify(g2?.warnings)}`);
    }
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（fit_background_field）`);
  process.exit(1);
}
console.log("\n  ✓ 掩膜内被平滑场替换、残差可查、掩膜外未动");
