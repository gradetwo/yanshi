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
    // **只查「单色」这一类** ✗→✓（第 1470 轮）：
    //   **⇒ 我原来的断言**要求 `warnings` **完全为空** ⇒ 而**这条渐变阶梯底 degree 2 也会过冲**
    //     （实测 **35361** 个通道被夹紧）⇒ **过冲警告是对的** ⇒ **原来是断言太严** ✓
    //   **⇒ ⇒ 所以**只断言「**没有单色警告**」+「uniform=false」 ✓（关注点分离 ✓）
    const uniformityWarned = Array.isArray(g2?.warnings)
      && g2.warnings.some((x) => /单色/.test(String(x)));
    if (g2?.field_uniform === false && !uniformityWarned) {
      ok(`⑥ 渐变层**不误报单色**（range ${JSON.stringify(g2.field_range)}、非单色警告 ${Array.isArray(g2?.warnings) ? g2.warnings.length : 0} 条）`);
    } else {
      bad(`⑥ 渐变层不该被判成单色 ⇒ 实测 uniform=${g2?.field_uniform}、单色警告=${uniformityWarned}`);
    }
  }
}

// ⑦ **`degree=0` = 用掩膜外的均值填充**（第 1458 轮）
//   **⇒ 为什么要有它** ✓：有人**不想要任何渐变**、只要"和周围一致的平均色" ⇒ 0 阶多项式**正好**是这个 ✓
//     （之前被 `clamp(1,3)` 静默挡掉 ⇒ 用户拿到的是**平面**而不是**均值** ⇒ 不对但看不出来 ✗）
//   **⇒ ⇒ 断什么**（**两个结构量**）：
//     ① `field_uniform === true` **且** `field_range` 两端相等 ⇒ "填的是单值" ✓
//     ② **残差 = 样本标准差**：rms 必须 **> 40**（我的阶梯底实测 76.5）⇒ 若 `clamp` 又回到 1，
//        rms 会掉到 73 附近（同样高 ✗）⇒ **所以再加**：`degree=0` 的 rms 必须 **≥** `degree=2` 的 rms ✓
//        （恒等式优于单点 ✓：阶数越低，残差**不可能更小**）
{
  const docZ = `bgf_d0_${stamp}`;
  const madeZ = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docZ, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeZ.token) {
    console.log("  ○ ⑦ 未验（建不出文档）");
  } else {
    const pZ = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docZ}&token=${madeZ.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pZ("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 12; k += 1) {
      const y = 10 + k * 23;
      const g = Math.round(255 * ((y / 300 - 0.5) ** 2 * 4));
      await pZ("fill_region", { layer_id: "L", shape: { type: "rect", x: 10, y, w: 380, h: 23 },
        color: [g, g, g], opacity: 1.0 });
    }
    const maskZ = { type: "rect", x: 120, y: 80, w: 160, h: 140 };
    const z0 = await pZ("fit_background_field", { layer_id: "L", mask: maskZ, degree: 0 });
    const z2 = await pZ("fit_background_field", { layer_id: "L", mask: maskZ, degree: 2 });
    const flat = Array.isArray(z0?.field_range) && z0.field_range[0] === z0.field_range[1];
    if (z0?.degree === 0 && flat) {
      ok(`⑦ degree=0 ⇒ 生效 0 且填的是**单值**（range ${JSON.stringify(z0.field_range)}）`);
    } else {
      bad(`⑦ degree=0 必须生效并填单值 ⇒ 实测 degree=${z0?.degree}、range=${JSON.stringify(z0?.field_range)}`);
    }
    const r0 = z0?.residual_rms?.[0];
    const r2 = z2?.residual_rms?.[0];
    if (typeof r0 === "number" && typeof r2 === "number" && r0 >= r2) {
      ok(`⑦ 恒等式成立：0 阶残差 ${r0.toFixed(1)} ≥ 2 阶残差 ${r2.toFixed(1)}（阶数越低残差不可能更小）`);
    } else {
      bad(`⑦ 0 阶残差必须 ≥ 2 阶 ⇒ 实测 0:${r0}、2:${r2}（若 0 阶被夹成 1 阶，这条会红）`);
    }
  }
}

// ⑧ **`sample_stride` 与 `margin` 必须**真的被读**（第 1460 轮；AGENTS 那条问句）
//   **⇒ 为什么** ✗：两个参数**都写在 spec 里**，但**前七条断言一条也没证明它们被读过** ⇒
//     **把它们静默忽略**（写死 stride=2、margin=0.25）⇒ **前七条照样全过** ✗
//   **⇒ ⇒ 实测**（同一构型只改一个参数）：
//     · `stride` 1 → 8 ⇒ **samples 112000 → 1780（~63×）**
//     · `margin` 0 → 1.0 ⇒ **samples 22784 → 52000（2.3×）**
//   **⇒ ⇒ ⇒ 断言**（**比值/方向**，不是写死的数字 ✓）：
//     ① `samples(stride=8)` **<** `samples(stride=1) / 10`（步长 8 倍 ⇒ 样本至少少一个量级）
//     ② `samples(margin=1.0)` **>** `samples(margin=0.0)`（外扩更大 ⇒ 样本更多）
//     ③ **恒等式**：`rms(stride=1)` **≤** `rms(stride=8)`（样本多不可能拟合更差）
{
  const docS = `bgf_par_${stamp}`;
  const madeS = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docS, width: 600, height: 400,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeS.token) {
    console.log("  ○ ⑧ 未验（建不出文档）");
  } else {
    const pS = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docS}&token=${madeS.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pS("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 12; k += 1) {
      const y = 10 + k * 30;
      const g = Math.round(255 * ((y / 400 - 0.5) ** 2 * 4));
      await pS("fill_region", { layer_id: "L", shape: { type: "rect", x: 10, y, w: 580, h: 30 },
        color: [g, g, g], opacity: 1.0 });
    }
    const maskS = { type: "rect", x: 200, y: 120, w: 200, h: 160 };
    const s1 = await pS("fit_background_field", { layer_id: "L", mask: maskS, degree: 2, sample_stride: 1 });
    const s8 = await pS("fit_background_field", { layer_id: "L", mask: maskS, degree: 2, sample_stride: 8 });
    const m0 = await pS("fit_background_field", { layer_id: "L", mask: maskS, degree: 2, margin: 0.0 });
    const m1 = await pS("fit_background_field", { layer_id: "L", mask: maskS, degree: 2, margin: 1.0 });
    if ((s8?.samples ?? 0) * 10 < (s1?.samples ?? 0)) {
      ok(`⑧ sample_stride 被读：stride 1 ⇒ ${s1.samples} 样本｜stride 8 ⇒ ${s8.samples}（少 10 倍以上）`);
    } else {
      bad(`⑧ sample_stride 必须影响采样数 ⇒ 实测 1:${s1?.samples}、8:${s8?.samples}（被忽略时两者相等）`);
    }
    if ((m1?.samples ?? 0) > (m0?.samples ?? 0)) {
      ok(`⑧ margin 被读：0 ⇒ ${m0.samples} 样本｜1.0 ⇒ ${m1.samples}（外扩更大 ⇒ 样本更多）`);
    } else {
      bad(`⑧ margin 必须影响采样数 ⇒ 实测 0:${m0?.samples}、1.0:${m1?.samples}`);
    }
    const q1 = s1?.residual_rms?.[0];
    const q8 = s8?.residual_rms?.[0];
    if (typeof q1 === "number" && typeof q8 === "number" && q1 <= q8 + 0.5) {
      ok(`⑧ 恒等式成立：stride 1 残差 ${q1.toFixed(1)} ≤ stride 8 残差 ${q8.toFixed(1)}（样本多不可能更差）`);
    } else {
      bad(`⑧ stride 1 的残差不应大于 stride 8 ⇒ 实测 1:${q1}、8:${q8}`);
    }
  }
}

// ⑨ **掩膜形状与**解析面积**相符（第 1464 轮；四种形状）
//   **⇒ 为什么** ✓：只断言 `filled > 0` 证明不了"掩膜被**忠实**使用" ⇒ 填充数**等于**解析面积才行 ✓
//   **⇒ ⇒ 实测**（半径 60 的圆 ⇒ π·60² = **11310**，实测 **11304**；椭圆 70×50 ⇒ **10996**，实测 **11004**）⇒ **误差 ~0.1%** ✓
//   **⇒ ⇒ ⇒ 断言**（**恒等式**，不是写死的数）：`filled` 与解析面积的**相对误差 < 2%**
{
  const docC = `bgf_circ_${stamp}`;
  const madeC = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docC, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeC.token) {
    console.log("  ○ ⑨ 未验（建不出文档）");
  } else {
    const pC = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docC}&token=${madeC.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pC("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 10; k += 1) {
      await pC("fill_region", { layer_id: "L",
        shape: { type: "rect", x: 10, y: 10 + k * 28, w: 380, h: 28 },
        color: [40 + k * 18, 40 + k * 18, 40 + k * 18], opacity: 1.0 });
    }
    const R = 60;
    const circ = await pC("fit_background_field", {
      layer_id: "L", mask: { type: "circle", cx: 200, cy: 150, r: R }, degree: 2 });
    const analytic = Math.PI * R * R;
    const got = circ?.filled_pixels ?? 0;
    const rel = Math.abs(got - analytic) / analytic;
    if (circ?.ok === true && rel < 0.02) {
      ok(`⑨ 圆的填充像素数 = 解析面积：实测 ${got} vs π·${R}² = ${analytic.toFixed(0)}（误差 ${(rel * 100).toFixed(2)}% < 2%）`);
    } else {
      bad(`⑨ 圆的填充像素数应与解析面积相符 ⇒ 实测 ${got} vs ${analytic.toFixed(0)}（误差 ${(rel * 100).toFixed(1)}%）`);
    }
    // 多边形（矩形顶点）⇒ 面积应精确等于 120x100
    const poly = await pC("fit_background_field", {
      layer_id: "L", degree: 2,
      mask: { type: "polygon", points: [[140, 100], [260, 100], [260, 200], [140, 200]] } });
    const rectArea = 120 * 100;
    const gotP = poly?.filled_pixels ?? 0;
    const relP = Math.abs(gotP - rectArea) / rectArea;
    if (poly?.ok === true && relP < 0.02) {
      ok(`⑨ 多边形（矩形四点）⇒ 实测 ${gotP} vs ${rectArea}（误差 ${(relP * 100).toFixed(2)}%）`);
    } else {
      bad(`⑨ 多边形面积应与顶点相符 ⇒ 实测 ${gotP} vs ${rectArea}`);
    }
  }
}

// ⑩ **过冲必须**说出来**（第 1470 轮；与「单色场」同一类诊断）
//   **⇒ 为什么** ✗：三阶多项式在**陡边**上**必然过冲**（值跑出 0-255）⇒ 我**静默 clamp** ✗
//     ⇒ 调用方以为"填的是拟合值"，实际**部分像素是夹紧值** ⇒ 与 `§222` 同类的静默 ✓
//   **⇒ ⇒ 实测**：陡边（一半 0、一半 255）+ `degree=3` ⇒ **`clamped_pixels=1893`** ✓
//     （`degree=1` 与平滑底 ⇒ **0** ⇒ 所以它不是恒真 ✓）
{
  const docO = `bgf_over_${stamp}`;
  const madeO = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docO, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeO.token) {
    console.log("  ○ ⑩ 未验（建不出文档）");
  } else {
    const pO = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docO}&token=${madeO.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pO("create_layer", { layer_id: "L", name: "L" });
    // 陡边：上半 0、下半 255
    await pO("fill_region", { layer_id: "L", shape: { type: "rect", x: 0, y: 10, w: 400, h: 150 },
      color: [0, 0, 0], opacity: 1.0 });
    await pO("fill_region", { layer_id: "L", shape: { type: "rect", x: 0, y: 160, w: 400, h: 130 },
      color: [255, 255, 255], opacity: 1.0 });
    const maskO = { type: "rect", x: 150, y: 100, w: 100, h: 80 };
    const o3 = await pO("fit_background_field", { layer_id: "L", mask: maskO, degree: 3, margin: 0.5 });
    const said = Array.isArray(o3?.warnings) && o3.warnings.some((x) => /超出 0-255/.test(String(x)));
    if ((o3?.clamped_pixels ?? 0) > 0 && said) {
      ok(`⑩ 陡边 + degree 3 ⇒ 过冲被**数出来并说出来**：clamped_pixels=${o3.clamped_pixels}，range ${JSON.stringify(o3.field_range)}`);
    } else {
      bad(`⑩ 陡边 + degree 3 应过冲并报出 ⇒ 实测 clamped=${o3?.clamped_pixels}、warnings=${JSON.stringify(o3?.warnings)}`);
    }
    // 反证：平缓底 + degree 1 ⇒ 不许瞎报
    const o1 = await pO("fit_background_field", { layer_id: "L", mask: maskO, degree: 1, margin: 0.5 });
    if ((o1?.clamped_pixels ?? -1) === 0) {
      ok(`⑩ 反证：degree 1 在同一个底上 clamped_pixels=0（不是恒真）`);
    } else {
      bad(`⑩ degree 1 不该过冲 ⇒ 实测 clamped=${o1?.clamped_pixels}`);
    }
  }
}

// ⑪ **被夹紧的参数必须**报生效值**（第 1472 轮）
//   **⇒ 为什么** ✗：`degree` 一直报生效值 ✓，但 `sample_stride`（>16 夹到 16）与 `margin`（>2 夹到 2）
//     **原来静默** ⇒ 调用方以为用了自己给的值 ✗（与 `§222` 同类：**静默降级**）
//   **⇒ ⇒ 实测**：请求 `degree=9 / stride=99 / margin=9.0` ⇒ 生效 **3 / 16 / 2.0** ✓
{
  const docE = `bgf_eff_${stamp}`;
  const madeE = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docE, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeE.token) {
    console.log("  ○ ⑪ 未验（建不出文档）");
  } else {
    const pE = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docE}&token=${madeE.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pE("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 10; k += 1) {
      await pE("fill_region", { layer_id: "L",
        shape: { type: "rect", x: 10, y: 10 + k * 28, w: 380, h: 28 },
        color: [40 + k * 18, 40 + k * 18, 40 + k * 18], opacity: 1.0 });
    }
    const e = await pE("fit_background_field", {
      layer_id: "L", mask: { type: "rect", x: 150, y: 100, w: 100, h: 80 },
      degree: 9, sample_stride: 99, margin: 9.0 });
    if (e?.degree === 3 && e?.sample_stride === 16 && e?.margin === 2.0) {
      ok(`⑪ 三个越界参数都被夹紧**且报生效值**：请求 9/99/9.0 ⇒ 生效 ${e.degree}/${e.sample_stride}/${e.margin}`);
    } else {
      bad(`⑪ 越界参数必须报生效值 ⇒ 实测 degree=${e?.degree}、stride=${e?.sample_stride}、margin=${e?.margin}`);
    }
  }
}

// ⑫ **样本波动与相对残差**必须报出，且**比值不等于 1**（第 1475 轮；外部 §224 的提问）
//   **⇒ 外部问**：`< 15` 的绝对阈值在**低对比**图上是否偏严？
//   **⇒ ⇒ 实测（同一条二次曲线，只缩放对比）**：
//     高对比 0..255 ⇒ `rms=30.6`、`std=79.5` ⇒ **ratio 0.385**
//     低对比 100..140 ⇒ `rms=24.1`、`std=30.0` ⇒ **ratio 0.804**
//   **⇒ ⇒ ⇒ 所以**：**比值也不能归一化**（低对比**更难**拟合 ⇒ 相对误差**更大**）✓
//     ⇒ 而外部 `test1`（`std 13.5`、`rms 11`）比值也是 **0.81** ⇒ **与他们判为可接受的情形同量级** ✓
//   **⇒ ⇒ ⇒ ⇒ 结论**：**没有单一判据** —— `< 15` 的**绝对**阈值是**对的**（误差的**可感知性**是绝对的），
//     而 `sample_std` / `residual_ratio` 只是**额外语境** ⇒ 断言只查"报出来了"与"比值合理"，不查阈值 ✓
{
  const docRt = `bgf_ratio_${stamp}`;
  const madeRt = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docRt, width: 400, height: 300,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!madeRt.token) {
    console.log("  ○ ⑫ 未验（建不出文档）");
  } else {
    const pRt = async (tool, a) => {
      const rr = await fetch(`${base}/api/tools/${tool}?doc=${docRt}&token=${madeRt.token}`, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(a ?? {}),
      });
      return rr.json();
    };
    await pRt("create_layer", { layer_id: "L", name: "L" });
    for (let k = 0; k < 10; k += 1) {
      const t = k / 9;
      const v = Math.round(255 * t * t);
      await pRt("fill_region", { layer_id: "L",
        shape: { type: "rect", x: 10, y: 10 + k * 28, w: 380, h: 28 },
        color: [v, v, v], opacity: 1.0 });
    }
    const rr = await pRt("fit_background_field", {
      layer_id: "L", mask: { type: "rect", x: 150, y: 100, w: 100, h: 80 }, degree: 2 });
    const rms = rr?.residual_rms?.[0];
    const sd = rr?.sample_std?.[0];
    const rat = rr?.residual_ratio?.[0];
    const okShape = typeof rms === "number" && typeof sd === "number" && typeof rat === "number"
      && sd > 0 && Math.abs(rat - rms / sd) < 0.01;
    if (okShape) {
      ok(`⑫ 报出 sample_std 与 residual_ratio 且**自洽**：rms ${rms.toFixed(1)} / std ${sd.toFixed(1)} = ${rat.toFixed(3)}`);
    } else {
      bad(`⑫ 必须报 sample_std 与 residual_ratio，且 ratio === rms/std ⇒ 实测 rms=${rms}、std=${sd}、ratio=${rat}`);
    }
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（fit_background_field）`);
  process.exit(1);
}
console.log("\n  ✓ 掩膜内被平滑场替换、残差可查、掩膜外未动");
