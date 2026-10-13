#!/usr/bin/env node
// **★ 图层组：组不透明度／可见性**向下继承** ✗ ★**（**第 252 轮 ✓）
//
// **∴ 判什么（**四条 ✗ 成对 ⇒ 有区分力 ✓）★**：
//   **∴ ① 组不透明度 ✗**：**建组 ⇒ 把 3 层移进去 ⇒ 组设 0.5** ⇒ **渲染结果必须与
//     "**同样 3 层 ✗ 每层各设 0.5 ✗ 不建组**"**逐像素相同** ✓
//     ＋ **∴ 前提（**写在明处 ✓）✗**：**三层的墨**不许重叠**（**∴ 否则"组 0.5"与
//       "逐层 0.5"在数学上**本来就不等** ✗ —— **∴ 那不是产品错 ✗ 是判据设计错** ✓）★★
//   **∴ ② 组可见性 ✗**：**组设 `visible:false` ⇒ **它下面的三层全消失**
//     （**∴ 画面上那三条带必须变回背景 ✓）★
//   **∴ ③ 撤销 ✗**：`undo_last` **一次 ⇒ 回到 ① 那张图**（**∴ 结构与像素一起回 ✓）★
//   **∴ ④ 有效样本 ✗**：**先要**真画上墨**✗ 且**三条带位置不重叠** ✓
//     （**∴ 否则"全白图"天然满足 ① ✗ 判据会假绿** ✓）★★
//
// **∴ 现状（**本轮读代码 ✓）✗**：**模型早有**（`Layer.parent_id` ＋
//   `LayerType::LayerGroup` ✓）✗ **`create_layer` 也收 `type`／`parent_id`** ✓
//     ＋ **∴ 而** `update_layer` 的 `patch` **不收 `parent_id`** ✗
//       ⇒ **∴ 所以**："**把已有图层移进组**"这一步**可能缺失** ✓ ⇒ **∴ 本判据**先把它问出来** ✓
//
// 用法：node scripts/tool-layer-group.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-layer-group.mjs <base-url>");
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

const W = 256;
const H = 256;
/** **三条**互不重叠**的横带 ✗**（**∴ ① 的等价性靠它 ✓）★ */
const BANDS = [
  { y: 40, color: { r: 230, g: 40, b: 40, a: 255 } },
  { y: 120, color: { r: 40, g: 200, b: 60, a: 255 } },
  { y: 200, color: { r: 40, g: 60, b: 230, a: 255 } },
];

const main = async () => {
  const fetchRaw = async (value) => {
    const url = value && (value.raw_url || value.rawUrl);
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const response = await fetch(absolute);
    if (!response.ok) return null;
    return new Uint8Array(await response.arrayBuffer());
  };
  /** **两张图的最大通道差 ✗**（**∴ 0 ⇒ 逐像素相同 ✓）★ */
  const maxDiff = (a, b) => {
    if (!a || !b || a.length !== b.length) return null;
    let worst = 0;
    for (let i = 0; i < a.length; i++) {
      const d = Math.abs(a[i] - b[i]);
      if (d > worst) worst = d;
    }
    return worst;
  };
  const makeDoc = async (suffix) => {
    const doc = `lg_${suffix}_${Date.now().toString(36)}`;
    const created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: W, height: H }),
      })
    ).json();
    if (!created || !created.token) return null;
    return { doc, token: created.token };
  };
  const drawBands = async ({ doc, token }) => {
    for (let i = 0; i < BANDS.length; i++) {
      const layerId = `L${i}`;
      await call(doc, token, "create_layer", { layer_id: layerId, name: layerId, z_index: 10 + i });
      const band = BANDS[i];
      await call(doc, token, "brush_stroke", {
        layer_id: layerId,
        object_id: `band${i}`,
        brush: "100%_Opaque",
        size: 28,
        points: [
          [40, band.y, 1.0],
          [W - 40, band.y, 1.0],
        ],
        color: band.color,
        preview: false,
      });
    }
  };
  const render = async ({ doc, token }) => {
    const value = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: W, h: H },
      raw: true,
    });
    return fetchRaw(value);
  };
  /** **背景色（**白 ✓）✗**（**∴ 用来断言"带子消失了" ✓）★ */
  const isBackgroundAt = (pixels, x, y) => {
    const i = (y * W + x) * 4;
    return pixels[i] >= 250 && pixels[i + 1] >= 250 && pixels[i + 2] >= 250;
  };
  const bandsInked = (pixels) => {
    let ok = 0;
    for (const band of BANDS) {
      if (!isBackgroundAt(pixels, W >> 1, band.y)) ok += 1;
    }
    return ok;
  };

  // **∴ A 组：建 3 层 ＋ 一条带 ✗ 再建组并把三层移进去** ✓
  const a = await makeDoc("group");
  const b = await makeDoc("flat");
  if (!a || !b) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  await drawBands(a);
  await drawBands(b);
  const before = await render(a);
  if (!before || bandsInked(before) !== BANDS.length) {
    console.error(`❌ 判不了：三条带没有都画上（inked=${before ? bandsInked(before) : "无图"}）⇒ 退出码 3`);
    process.exit(3);
  }

  // **∴ 建组 ✗**（**`create_layer` 收 `type: layer_group` ✓）★
  const group = await call(a.doc, a.token, "create_layer", {
    layer_id: "G1",
    name: "G1",
    type: "layer_group",
    z_index: 5,
  });
  if (!group || group.ok !== true) {
    console.error(`❌ 判不了：建不了图层组（${JSON.stringify(group).slice(0, 200)}）⇒ 退出码 3`);
    process.exit(3);
  }
  // **∴ 把三层移进组 ✗**（**∴ 这一步就是本判据要问的 ✓）★
  const moved = [];
  for (const layerId of ["L0", "L1", "L2"]) {
    moved.push(await call(a.doc, a.token, "update_layer", { layer_id: layerId, patch: { parent_id: "G1" } }));
  }
  const moveFailed = moved.find((r) => !r || r.ok !== true);
  if (moveFailed) {
    console.error(
      `❌ **把已有图层移进图层组**这一步不成立：${JSON.stringify(moveFailed).slice(0, 220)}`,
    );
    console.error("   ⇒ 这是产品的真实缺口（`update_layer` 的 patch 不收 `parent_id`）");
    process.exit(1);
  }
  // **∴ 组不透明度 0.5 ✗** ／ **∴ B 组：三层各 0.5 ✗ 不建组** ✓
  const setGroup = await call(a.doc, a.token, "update_layer", { layer_id: "G1", patch: { opacity: 0.5 } });
  if (!setGroup || setGroup.ok !== true) {
    console.error(`❌ 判不了：设不了组不透明度（${JSON.stringify(setGroup).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  for (const layerId of ["L0", "L1", "L2"]) {
    await call(b.doc, b.token, "update_layer", { layer_id: layerId, patch: { opacity: 0.5 } });
  }
  const grouped = await render(a);
  const flat = await render(b);
  const diff = maxDiff(grouped, flat);
  const bad = [];
  if (diff === null) bad.push("① 取不回两张图（判据自己坏了）");
  else if (diff !== 0) bad.push(`① 组不透明度 0.5 应等于逐层 0.5 ✗（实测最大通道差 ${diff}）`);

  // **∴ ② 组隐藏 ⇒ 三条带消失 ✗** ✓
  await call(a.doc, a.token, "update_layer", { layer_id: "G1", patch: { visible: false } });
  const hidden = await render(a);
  if (!hidden) bad.push("② 取不回隐藏后的图（判据自己坏了）");
  else if (bandsInked(hidden) !== 0) {
    bad.push(`② 组隐藏后它下面三层应全部消失 ✗（还有 ${bandsInked(hidden)} 条带在）`);
  }

  // **∴ ③ 撤销一次 ⇒ 回到 ① 那张 ✗** ✓
  const undone = await call(a.doc, a.token, "undo_last", {});
  if (!undone || undone.ok !== true) {
    bad.push(`③ 撤销不成立（${JSON.stringify(undone).slice(0, 160)}）`);
  } else {
    const restored = await render(a);
    const undoDiff = maxDiff(restored, grouped);
    if (undoDiff === null) bad.push("③ 取不回撤销后的图（判据自己坏了）");
    else if (undoDiff !== 0) bad.push(`③ 撤销后应回到组不透明度 0.5 那张 ✗（实测最大通道差 ${undoDiff}）`);
  }

  console.log(`  ① 组 0.5 vs 逐层 0.5：最大通道差 = ${diff}`);
  console.log(`  ② 组隐藏后仍在的带数 = ${hidden ? bandsInked(hidden) : "?"}（要求 0）`);
  console.log(`  ③ 撤销：${undone && undone.ok ? "成功" : "失败"}`);
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 组不透明度向下继承、组隐藏连带隐藏、撤销可回退（三条都成立）");
};

await main();
