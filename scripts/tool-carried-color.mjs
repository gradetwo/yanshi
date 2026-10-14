#!/usr/bin/env node
// **★ carried_color：脏笔拾取（湿画法）★**（第 254 轮，第三版 —— 前两版都是**量错了**）
//
// **需求文档 P0 #1**："引擎 carried_color（脏笔拾取 ⇒ 湿画法，Blender 复活）"，
// 现状写的是"Blender 只能 Source-Over"。硬约束里有"不作弊"、确定性等。
//
// **核实与实现（结论写在台账里）**：
//   * 读代码：链路本来就是接上的（`feed_base` / `reads_canvas`），内核 `hokusai-core 0.3.0`
//     的揉合也是真实现（`brushmodes.rs` 有 smudge bucket cache）；
//   * 但在这条落笔路径上**跨色边界一个过渡色都没有**（实测：跨过红带边缘，带外仍纯白），
//     参数怎么调都一样 => **决策：在我们自己的渲染层做净室实现**
//     （`yanshi-render::brush::smudge_dab` ＋ 在 `stamp_stroke_from` 里逐 dab 调用，
//      只对 `smudge > 0` 的笔刷生效）；
//   * 实测已生效：从**带内往带外**走时，红被拖到带外（见下面的数字）。
//
// **★ 判据前两版为什么量不到（都写下来，避免重犯）★**：
//   1. 第一版沿**均匀红带的轴向**涂抹 => 均匀色沿自身平移**看不出变化**（只有边界会动）；
//   2. 第二版方向反了（"带外→带内"时后方是**白** => 效果是把白拖进红带，带外当然没有红）＋
//      统计区域把**红带自己的首行**算进"带外"（基线非零、失去区分力）。
//   => 现在：**宽色带（96px）** ＋ **从带中心往外拖** ＋ `smudge_length: 3.0`（合法参数）
//      => 拖出来的红落在**明确带外**（实测带外红像素 1412、y=120 处为纯红 rgb(220,30,30)）。
//   * 为什么必须调 `smudge_length`：它是**相对半径**的比例（MyPaint 语义），默认 0.5
//     => 拖曳仅 ~12px，落在抗锯齿边缘的 1–2 行里，量不稳。
//
// **判什么（五条）**：
//   ① 底稿（96px 红带）在带外**没有**红像素（前提）；
//   ② `Blender` 从带中心往带外拖之后，**带外必须出现红**（脏笔拾取把颜料带出来了）；
//   ③ 带色揉合（`blend+paint` 在红底上画蓝）⇒ 区域均值里**红分量明显高于纯蓝**（拾取生效）；
//   ④ 阴性对照：同一条几何用**非揉合**笔刷 => 带外不得出现红（判据有区分力）；
//   ⑤ 确定性：同配方两次**逐字节一致**。
//
// **变异（能红的根据）**：把 `smudge_dab` 的调用删掉（或 `amount` 恒 0）=> ② 立刻红。
//
// 用法：node scripts/tool-carried-color.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-carried-color.mjs <base-url>");
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

let seq = 0;
const newDoc = async (tag) => {
  seq += 1;
  const doc = `cc_${tag}_${Date.now().toString(36)}_${seq}`;
  const response = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 384, height: 384 }),
  });
  const text = await response.text();
  let created = null;
  try {
    created = JSON.parse(text);
  } catch {
    console.error(`❌ 判不了：/api/documents 回的不是 JSON（HTTP ${response.status}）：${text.slice(0, 120)}`);
    process.exit(3);
  }
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  await call(doc, created.token, "create_layer", { layer_id: "L1", name: "L1" });
  return { doc, token: created.token };
};

// 96px 高的红带：中心 y=208 => 带覆盖 y≈160..256
const BAND_Y = 208;
const BAND_TOP = 160;
const RED = { r: 220, g: 30, b: 30, a: 255 };

const readRegion = async (doc, token) => {
  const raw = await call(doc, token, "render_region", {
    region: { x: 0, y: 0, w: 384, h: 384 },
    raw: true,
  });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) return null;
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
  const W = Number(raw.width || 384);
  const H = Number(raw.height || 384);
  if (W * H * 4 !== bytes.length) return null;
  const pixel = (x, y) => {
    const at = (y * W + x) * 4;
    return [bytes[at], bytes[at + 1], bytes[at + 2]];
  };
  const isRed = (x, y) => {
    const [r, g, b] = pixel(x, y);
    return r > 120 && r > g + 50 && r > b + 50;
  };
  let fingerprint = 0;
  for (let i = 0; i < bytes.length; i += 4) {
    fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
  }
  return { W, H, pixel, isRed, fingerprint };
};

/** **严格带外**（`y < BAND_TOP`）的红像素数 */
const redAbove = (view) => {
  let count = 0;
  for (let y = 0; y < BAND_TOP; y += 1) {
    for (let x = 0; x < view.W; x += 1) {
      if (view.isRed(x, y)) count += 1;
    }
  }
  return count;
};

const paintBand = (doc, token, objectId) =>
  call(doc, token, "brush_stroke", {
    layer_id: "L1",
    object_id: objectId,
    brush: "100%_Opaque",
    size: 96,
    color: RED,
    points: [[60, BAND_Y], [324, BAND_Y]],
  });

const main = async () => {
  const bad = [];

  // ① 底稿 + ② Blender 从带中心往带外拖
  const drag = await newDoc("drag");
  await paintBand(drag.doc, drag.token, "band");
  const before = await readRegion(drag.doc, drag.token);
  if (!before) {
    console.error("❌ 判不了：量不到底稿渲染 ⇒ 退出码 3");
    process.exit(3);
  }
  const beforeAbove = redAbove(before);
  console.log(`  ① 底稿（96px 红带，上边缘约 y=${BAND_TOP}）：带外红像素 = ${beforeAbove}`);

  const smear = await call(drag.doc, drag.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "smear",
    brush: "Blender",
    size: 48,
    // **从带中心往上拖到带外**（后方＝红）
    points: [[192, BAND_Y, 1.0], [192, 100, 1.0]],
    brush_params: { smudge_length: 3.0 },
  });
  if (!smear || smear.ok !== true) {
    console.error(
      `❌ 判不了：Blender 那一笔没成功（${JSON.stringify(smear).slice(0, 160)}）⇒ 退出码 3`,
    );
    process.exit(3);
  }
  const after = await readRegion(drag.doc, drag.token);
  if (!after) {
    console.error("❌ 判不了：量不到揉合之后的渲染 ⇒ 退出码 3");
    process.exit(3);
  }
  const afterAbove = redAbove(after);
  console.log(
    `  ② Blender 拖出带外之后：带外红像素 = ${afterAbove}（底稿 ${beforeAbove}）｜取样 y=120 rgb(${after.pixel(192, 120).join(",")})`,
  );

  // ③ 带色揉合：红底上画蓝 => 红分量必须被带出来
  const mix = await newDoc("mix");
  await paintBand(mix.doc, mix.token, "band");
  await call(mix.doc, mix.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "paint",
    brush: "blend+paint",
    size: 48,
    color: { r: 20, g: 40, b: 220, a: 255 },
    points: [[60, BAND_Y], [324, BAND_Y]],
  });
  const mixed = await readRegion(mix.doc, mix.token);
  if (!mixed) {
    console.error("❌ 判不了：量不到带色揉合的渲染 ⇒ 退出码 3");
    process.exit(3);
  }
  let meanR = 0;
  let meanB = 0;
  let n = 0;
  for (let y = BAND_Y - 8; y < BAND_Y + 16; y += 1) {
    for (let x = 60; x < 320; x += 1) {
      const [r, , b] = mixed.pixel(x, y);
      meanR += r;
      meanB += b;
      n += 1;
    }
  }
  meanR /= Math.max(1, n);
  meanB /= Math.max(1, n);
  console.log(
    `  ③ 带色揉合（蓝画在红上）：区域均值 r=${meanR.toFixed(0)} b=${meanB.toFixed(0)}（纯蓝 r≈20）`,
  );

  // ④ 阴性对照：同几何、非揉合笔刷
  const plain = await newDoc("plain");
  await paintBand(plain.doc, plain.token, "band");
  const plainBefore = await readRegion(plain.doc, plain.token);
  await call(plain.doc, plain.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "plain",
    brush: "100%_Opaque",
    size: 48,
    color: { r: 20, g: 40, b: 220, a: 255 },
    points: [[192, BAND_Y, 1.0], [192, 100, 1.0]],
  });
  const plainAfter = await readRegion(plain.doc, plain.token);
  const plainAbove = redAbove(plainAfter);
  console.log(
    `  ④ 阴性对照（非揉合笔刷同几何）：带外红像素 = ${plainAbove}（底稿 ${redAbove(plainBefore)}）`,
  );

  // ⑤ 确定性：同配方两次
  const again = await newDoc("again");
  await paintBand(again.doc, again.token, "band");
  await call(again.doc, again.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "smear",
    brush: "Blender",
    size: 48,
    points: [[192, BAND_Y, 1.0], [192, 100, 1.0]],
    brush_params: { smudge_length: 3.0 },
  });
  const againView = await readRegion(again.doc, again.token);

  // 判定
  if (beforeAbove !== 0) {
    bad.push(`① 底稿在带外就有红（${beforeAbove}）⇒ 前提不成立`);
  }
  if (afterAbove <= 0) {
    bad.push(
      "② `Blender` 从带中心往带外拖之后，带外**没有**红 ⇒ 脏笔拾取没把颜料带出来" +
        "（把 `smudge_dab` 的调用删掉、或 amount 为 0 时就会这样）",
    );
  }
  if (meanR < 60) {
    bad.push(`③ 带色揉合没有把红带出来（区域均值 r=${meanR.toFixed(0)}，要求 ≥60）`);
  }
  if (plainAbove > 0) {
    bad.push(`④ 阴性对照失败：非揉合笔刷也把红带到带外（${plainAbove}）⇒ 判据没有区分力`);
  }
  if (!againView || againView.fingerprint !== after.fingerprint) {
    bad.push(
      `⑤ 同配方两次渲染不一致（${after ? after.fingerprint : "?"} vs ${againView ? againView.fingerprint : "?"}）`,
    );
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 脏笔拾取生效：纯揉合把红拖到带外（${beforeAbove} ⇒ ${afterAbove} 个红像素，y=120 处 rgb(${after.pixel(192, 120).join(",")})），` +
      `带色揉合混出红蓝同在（r=${meanR.toFixed(0)}），阴性对照 0，且可复现`,
  );
};

await main();
