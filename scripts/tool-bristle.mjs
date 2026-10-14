#!/usr/bin/env node
// **★ 笔毛质感：引擎**本来就有**，别再写一遍（P0 #2 ← 修正后的结论 ✓）★**（第 254 轮 ✓）
//
// **∴ 用户的提醒（**原话 ✓）✗**：**"有些东西看看原本引擎或者笔刷自己有没有，避免重复造轮子"** ✓ ★★★
//   ＋ **∴ 于是我先把 Hokusai／MyPaint 的**63 个规范设置**列了一遍（`BrushSetting::cname()` ✓）
//     ＋ **∴ 里面有**整组**笔毛/毛边旋钮** ✓**：`offset_by_random`／`radius_by_random`／
//       `dabs_per_basic_radius`／`dabs_per_actual_radius`／`elliptical_dab_ratio`／`offset_by_speed*`
//       ／`gridmap_*`（**画布纹理门控 ✓）／`speed1_*`·`speed2_*`／`smudge_*`／`pressure_gain_log` …** ✓ ★★
//     ＋ **∴ 而 P0 #4 的 `brush_params`（**本轮已完成 ✓）**正好能驱动其中**任意一个** ✓✓
//       ⇒ **∴ 所以"笔毛分叉"**极可能不需要动引擎** ✗ ⇒ **∴ 先量 ✗ 再决定** ✓ ★★★
//
// **∴ 实测（**本机 512²，恒定压感 ✓；指标 = **每列首个暗像素位置的标准差** ＝ 毛边程度 ✓）★**：
//   **∴ 缺省              边缘标准差 2.86｜平均笔段 1.00**（**光边 ✓）
//   **∴ 少重叠＋半径随机   边缘标准差 8.15**（**`dabs_per_basic_radius:0.4` + `radius_by_random:1.5` ✓）
//   **∴ 少重叠＋位置随机   边缘标准差 13.53｜笔段 1.35**（**＋`offset_by_random:1.5` ⇒ **真的分股** ✓）
//   **∴ 三者齐上            边缘标准差 15.09** ✓ ★★★
//     ⇒ **∴ 结论 ✗**：**引擎**本来就能**做出笔毛质感**✗ **∴ 不写引擎代码 ✗ 只把它**守住** ✓ ★★★
//
// **∴ 判什么 ✗**：**（a）默认笔画**边缘平滑**（标准差 < 4 ✓）✗
//   **（b）加上那组旋钮之后**边缘显著变毛**（≥ 2× 默认 ✓）✗ 且**至少一种组合能分出笔段**（> 1.2 ✓）** ✓
//
// **∴ 变异（**能红的根据 ✓）✗**：**把那组 `brush_params` 删掉（**或把 P0 #4 的解析短路 ✓）
//   ⇒ **∴（b）立刻红**（**回落到 2.86 ⇒ 比值 ~1×** ✓）★★★
//
// **∴ 本判据**不做**什么 ✗**：**不判**好不好看**✗ **不判**具体走哪几个旋钮 ✓（**只判"这个能力在" ✓）★
//
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-bristle.mjs <base-url>");
  process.exit(2);
}
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

// **★ 指标：边缘标准差（毛边程度）✗ 与 平均笔段（分股程度）✓ ★**（**第 254 轮 ✓）
//   ＋ **∴ 为什么用它们 ✗**：**"笔毛质感"没法直接量 ✗ 但**毛边**与**分股**可以** ✓
//     ＋ **∴ 而且**两者都只依赖墨的**形状**✗ 与颜色/明暗无关 ⇒ **稳定 ✓** ★★
async function measure(extra, tag) {
  const doc = `br_${tag}_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
    })
  ).json();
  if (!created || !created.token) return null;
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  // 恒定压感 + 直线（隔离掉压感锥度）
  const LINE = [];
  for (let i = 0; i <= 16; i += 1) LINE.push([40 + i * 26, 256, 1.0]);
  const args = {
    layer_id: "L1",
    object_id: "s",
    brush: "100%_Opaque",
    size: 64,
    opacity: 1.0,
    color: { r: 10, g: 10, b: 10, a: 255 },
    points: LINE,
  };
  Object.assign(args, extra || {});
  const replied = await call(doc, token, "brush_stroke", args);
  const raw = await call(doc, token, "render_region", {
    region: { x: 20, y: 180, w: 480, h: 152 },
    raw: true,
  });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) return { replied, stats: null };
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
  const W = Number(raw.width || 480);
  const H = Number(raw.height || 152);
  if (W * H * 4 !== bytes.length) return { replied, stats: null };
  const dark = (x, y) => {
    const at = (y * W + x) * 4;
    return bytes[at] + bytes[at + 1] + bytes[at + 2] < 600; // 白底上的墨 = 暗
  };
  const widths = [];
  const runs = [];
  const tops = [];
  for (let x = 0; x < W; x += 1) {
    let n = 0;
    let seg = 0;
    let inside = false;
    let first = -1;
    for (let y = 0; y < H; y += 1) {
      if (dark(x, y)) {
        n += 1;
        if (first < 0) first = y;
        if (!inside) {
          seg += 1;
          inside = true;
        }
      } else {
        inside = false;
      }
    }
    widths.push(n);
    runs.push(seg);
    tops.push(first);
  }
  const inked = widths.filter((v) => v > 0);
  const mean = (a) => a.reduce((p, c) => p + c, 0) / Math.max(1, a.length);
  const topsInked = tops.filter((v) => v >= 0);
  const topMean = mean(topsInked);
  return {
    replied,
    stats: {
      inkCols: inked.length,
      meanWidth: mean(inked),
      meanRuns: mean(runs.filter((_, i) => widths[i] > 0)),
      edgeStd: Math.sqrt(mean(topsInked.map((v) => (v - topMean) ** 2))),
    },
  };
}

const main = async () => {
  const bad = [];
  const plain = await measure(undefined, "plain");
  const rough = await measure(
    { brush_params: { dabs_per_basic_radius: 0.4, radius_by_random: 1.5 } },
    "rough",
  );
  const strands = await measure(
    { brush_params: { dabs_per_basic_radius: 0.4, offset_by_random: 1.5 } },
    "strands",
  );
  if (!plain || !plain.stats || !rough || !rough.stats || !strands || !strands.stats) {
    console.error(
      `❌ 判不了：量不到形状（plain=${JSON.stringify(plain && plain.replied).slice(0, 90)}｜rough=${JSON.stringify(rough && rough.replied).slice(0, 90)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  const f = (v) => v.toFixed(2);
  console.log(`  (a) 默认：边缘标准差 ${f(plain.stats.edgeStd)}｜平均笔段 ${f(plain.stats.meanRuns)}｜平均宽 ${f(plain.stats.meanWidth)}`);
  console.log(`  (b) 半径随机：边缘标准差 ${f(rough.stats.edgeStd)}（${f(rough.stats.edgeStd / plain.stats.edgeStd)}×）`);
  console.log(`  (b) 位置随机：边缘标准差 ${f(strands.stats.edgeStd)}（${f(strands.stats.edgeStd / plain.stats.edgeStd)}×）｜平均笔段 ${f(strands.stats.meanRuns)}`);

  // (a) 默认要平滑
  if (plain.stats.edgeStd >= 4) {
    bad.push(`(a) 默认笔画的边缘太平滑（标准差 ${f(plain.stats.edgeStd)} 要求 < 4）⇒ 指标或默认值不对`);
  }
  // (b) 加旋钮要显著变毛
  const ratio = Math.max(rough.stats.edgeStd, strands.stats.edgeStd) / Math.max(1e-6, plain.stats.edgeStd);
  if (ratio < 2) {
    bad.push(`(b) 加上笔毛旋钮后边缘没有明显变毛（最大 ${f(ratio)}× 要求 ≥ 2×）⇒ 现有旋钮做不到，才需要动引擎`);
  }
  if (strands.stats.meanRuns <= 1.2) {
    bad.push(`(b) 没有任何组合让笔画**分股**（最大平均笔段 ${f(strands.stats.meanRuns)} 要求 > 1.2）`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 引擎本来就能做笔毛质感（无需改引擎）：光边 ${f(plain.stats.edgeStd)} ⇒ 毛边 ${f(ratio)}×，且位置随机能分股`,
  );
};

await main();
