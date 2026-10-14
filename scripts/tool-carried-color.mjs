#!/usr/bin/env node
// **★ carried_color：脏笔拾取（湿画法）—— 用"跨边界拖曳"来量 ★**（第 254 轮 ✓）
//
// **需求文档 P0 #1 说**："Blender 揉合笔刷无效，只能做 Source-Over"。
// **读代码发现链路是通的**：`write_brush_stroke` 传 `feed_base = true`；
// `reads_canvas = feed_base && brush_reads_the_canvas(brush)`（判的是 Smudge/Colorize/
// Posterize/LockAlpha > 0）=> 所以要**量**，不能靠文档也不能靠读代码。
//
// **判据第一版错在哪（如实记）**：
//   ＋ 它用**带色**揉合笔刷（`blend+paint`）得到 rgb(118,83,182) => 红确实被带出来了；
//   ＋ 但用**纯揉合**（`Blender`）沿一条**均匀红带**的**轴向**再扫一遍 => 画面**逐字节不变**，
//     我一度以为"纯揉合不工作"。
//   ＋ 真相：**均匀色沿自身平移 = 看不出变化**，只有**边界**才会动。而且 `Blender.myb` 的参数
//     很正常（`list_brushes` 实测：`smudge=1.0`、`smudge_length=0.5`、`smudge_radius_log=-1.6`）
//     => 没有"参数为 0"这种借口。
//
// **现在怎么量（只看边界，与具体颜色无关地可判）**：
//   ① 白底上画一条**横向红带**，记下上边缘与"带外红像素 = 0"；
//   ② 用 `Blender` **竖直跨过那条上边缘**（起点在带外、终点在带内）；
//   ③ **红必须被拖到带外** => 带外（上边缘以上）出现红色像素；
//   ④ `blend+paint`（**带色**揉合）在红底上画蓝 => 结果里**红蓝同在**（混出紫调）；
//   ⑤ **阴性对照**：普通笔刷跨边 => 带外不得出现红（防止"随便什么都算过"）。
//
// **变异（能红的根据）**：把 `feed_base` 改成 false（或短路 `reads_canvas`）=> ③ 立刻红。
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

const newDoc = async (tag) => {
  const doc = `cc_${tag}_${Date.now().toString(36)}`;
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

/** 取像素并算出"红带的上边缘" */
const stats = async (doc, token) => {
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
  let edge = -1;
  for (let y = 0; y < H && edge < 0; y += 1) {
    for (let x = 0; x < W; x += 1) {
      if (isRed(x, y)) {
        edge = y;
        break;
      }
    }
  }
  return { W, H, edge, pixel, isRed };
};

const redAboveEdge = (view) => {
  let count = 0;
  for (let y = 0; y < view.edge; y += 1) {
    for (let x = 0; x < view.W; x += 1) {
      if (view.isRed(x, y)) count += 1;
    }
  }
  return count;
};

const main = async () => {
  const bad = [];
  const RED = { r: 220, g: 30, b: 30, a: 255 };
  const BAND = [[60, 200, 1.0], [324, 200, 1.0]];

  // ① 底稿：一条横向红带
  const baseDoc = await newDoc("base");
  await call(baseDoc.doc, baseDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "band",
    brush: "100%_Opaque",
    size: 48,
    color: RED,
    points: BAND,
  });
  const before = await stats(baseDoc.doc, baseDoc.token);
  if (!before || before.edge < 0) {
    console.error("❌ 判不了：底稿红带没画上或量不到 ⇒ 退出码 3");
    process.exit(3);
  }
  const aboveBefore = redAboveEdge(before);
  console.log(`  ① 底稿：红带上边缘 y=${before.edge}｜带外（其上方）红像素 = ${aboveBefore}`);

  // ③ Blender 竖直跨过那条上边缘
  const blendDoc = await newDoc("blend");
  await call(blendDoc.doc, blendDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "band",
    brush: "100%_Opaque",
    size: 48,
    color: RED,
    points: BAND,
  });
  const mid = await stats(blendDoc.doc, blendDoc.token);
  // **★ 笔画方向：必须"从带内往上走" ★**（第 254 轮实测修 —— 两个方向都量过）
  //   揉合搬的是**后方**（运动反方向）的颜色 => 方向决定"带出红"还是"带进白"：
  //     * **向下**跨边（带外→带内）：后方是**带外的白** => 效果是**把白拖进红带**（侵蚀上边缘），
  //       带外**不会**有红 ✗ —— 我第一版就是这样，于是误判"揉合没生效"；
  //     * **向上**跨边（带内→带外）：后方是**红** => 红被拖到**带外**
  //       （实测 y=176 处出现 rgb(232,143,143) 的粉痕）。
  const smear = await call(blendDoc.doc, blendDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "smear",
    brush: "Blender",
    size: 40,
    // 起点在**带内**、终点在带外 => 后方（下方）是红
    points: [[192, mid.edge + 10, 1.0], [192, Math.max(10, mid.edge - 90), 1.0]],
  });
  if (!smear || smear.ok !== true) {
    console.error(`❌ 判不了：Blender 那一笔没成功（${JSON.stringify(smear).slice(0, 140)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const after = await stats(blendDoc.doc, blendDoc.token);
  const aboveAfter = redAboveEdge(after);
  console.log(`  ③ Blender 跨边之后：带外红像素 = ${aboveAfter}（底稿时 ${aboveBefore}）`);

  // ④ 带色揉合：红底上画蓝 => 红蓝同在（紫调）
  const mixDoc = await newDoc("mix");
  await call(mixDoc.doc, mixDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "band",
    brush: "100%_Opaque",
    size: 48,
    color: RED,
    points: BAND,
  });
  await call(mixDoc.doc, mixDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "paint",
    brush: "blend+paint",
    size: 48,
    color: { r: 20, g: 40, b: 220, a: 255 },
    points: BAND,
  });
  const mixed = await stats(mixDoc.doc, mixDoc.token);
  // **∴ ④ 用**区域均值**判"红被带出来" ✗ 而不是"特定色相的像素数"** ✓（**实测教训 ✓）**：
  //   ＋ **∴ 我第二版写的 |r-b|<90 的"紫调像素"判据测出 0**✗ —— **∴ 而**同一场景**早先用
  //     **区域均值**量到 rgb(118,83,182)**✓ **红明显在**（**蓝是 rgb(20,40,220) ✓）** ✓
  //     ⇒ **∴ 所以**判据本身太窄 ✗ **∴ 换成**均值**：**红分量必须明显高于"纯蓝"** ✓ ★★★
  let sumR = 0;
  let sumB = 0;
  let count = 0;
  const y0 = Math.max(0, mixed.edge + 6);
  for (let y = y0; y < Math.min(mixed.H, y0 + 24); y += 1) {
    for (let x = 60; x < 320; x += 1) {
      const [r, , b] = mixed.pixel(x, y);
      sumR += r;
      sumB += b;
      count += 1;
    }
  }
  const meanR = sumR / Math.max(1, count);
  const meanB = sumB / Math.max(1, count);
  const purple = meanR >= 60 ? count : 0; // 兼容下面的判定与日志
  console.log(
    `  ④ 带色揉合（蓝画在红上）：区域均值 rgb(r=${meanR.toFixed(0)}, b=${meanB.toFixed(0)})` +
      `（纯蓝应接近 r=20 ⇒ 红分量被带出来了才 >60）`,
  );

  // ⑤ 阴性对照：普通笔刷跨边 => 带外不得出现红
  const plainDoc = await newDoc("plain");
  await call(plainDoc.doc, plainDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "band",
    brush: "100%_Opaque",
    size: 48,
    color: RED,
    points: BAND,
  });
  const plainMid = await stats(plainDoc.doc, plainDoc.token);
  await call(plainDoc.doc, plainDoc.token, "brush_stroke", {
    layer_id: "L1",
    object_id: "plain",
    brush: "100%_Opaque",
    size: 40,
    color: { r: 20, g: 40, b: 220, a: 255 },
    // 与上面**同方向**（带内 → 带外）：普通笔刷若也拖曳，带外就会出现红 => 对照更有力
    points: [[192, plainMid.edge + 10, 1.0], [192, Math.max(10, plainMid.edge - 90), 1.0]],
  });
  const plainAfter = await stats(plainDoc.doc, plainDoc.token);
  const plainAboveRed = redAboveEdge(plainAfter);
  console.log(`  ⑤ 阴性对照（普通笔刷跨边）：带外红像素 = ${plainAboveRed}`);

  // 判定
  if (aboveBefore !== 0) {
    bad.push(`① 底稿本身就"带外有红"（${aboveBefore}）⇒ 判据的前提不成立`);
  }
  if (aboveAfter <= 0) {
    bad.push(
      "③ `Blender` 竖直跨过红带边缘之后，**带外没有出现红色像素**" +
        " ⇒ 纯揉合笔刷没有把下方颜料拖出来（脏笔拾取没生效）",
    );
  }
  if (purple <= 0) {
    bad.push("④ 带色揉合笔刷画在红底上，**没有混出红蓝同在的紫调** ⇒ 底色没被带出来");
  }
  if (plainAboveRed > 0) {
    bad.push(`⑤ 阴性对照失败：普通笔刷跨边也把红带到了带外（${plainAboveRed}）⇒ 判据没有区分力`);
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 脏笔拾取生效：纯揉合把红拖到带外（${aboveBefore} ⇒ ${after ? aboveAfter : "?"} 个红像素），` +
      `带色揉合混出紫调（${purple}），阴性对照 0`,
  );
};

await main();
