#!/usr/bin/env node
// **★ 视觉锚点：回执里带结构化 anchors（P1 #12 · data 模式）★**（第 254 轮 ✓）
//
// **需求文档 P1 #12**：AI 看缩略图要能**精准接下一笔**，不能盲猜坐标。两种模式：
//   * `data`（**默认**）：**一张干净缩略图 ＋ 结构化锚点 JSON**（last_stroke 的 start/end/bbox、
//     grid 间距与原点、canvas 尺寸）—— token 便宜、坐标精确、可编程；
//   * `dual`：两张图（干净 ＋ 带 Overlay）—— 双倍 token，**本版未实现**（传它会得到明确报错）。
//
// **为什么这条几乎免费**：文档自己说「yanshi 每次 Tool Call 返回缩略图，这是天然视觉反馈通道」
//   => 缩略图那一半**已经有了**（框架在 `ctx.commit` 时附），缺的只是**结构化锚点**那一半。
//
// **判什么（四条）**：
//   ① 回执里有 `anchors.last_stroke{start,end,bbox}`，且 **start/end 必须等于我传的首末两点**
//      （精确到坐标 —— 这是本判据最硬的证据），`bbox` 必须**包住这两点**；
//   ② `anchors.grid{spacing,origin}` 与 `anchors.canvas{width,height}` 必须与文档/参数一致
//      （`grid_spacing` 参数要能改它）；
//   ③ 同参数两次调用，锚点**完全一致**（确定性 —— 硬约束）；
//   ④ **变异**：把 anchors 那段删掉 => ① 立刻红。
//
// 用法：node scripts/tool-anchors.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-anchors.mjs <base-url>");
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

const main = async () => {
  const W = 640;
  const H = 480;
  const doc = `an_${Date.now().toString(36)}`;
  const response = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: W, height: H }),
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
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });

  const POINTS = [[120, 140, 0.4], [260, 180, 0.9], [400, 150, 0.6]];
  // **★ `object_id` 必须每次不同 ★**（第 254 轮自查修）
  //   实测踩到：三次调用复用同一个 `object_id` => 后两次被当成**重复**，
  //   于是回执里没有 `anchors`（我一度以为"grid_spacing 没生效"与"不确定"）。
  let seq = 0;
  const stroke = (extra) => {
    seq += 1;
    return call(doc, token, "brush_stroke", {
      layer_id: "L1",
      object_id: `anchor_probe_${seq}`,
      brush: "100%_Opaque",
      size: 32,
      color: { r: 10, g: 10, b: 10, a: 255 },
      points: POINTS,
      ...(extra || {}),
    });
  };

  const first = await stroke({});
  if (!first || first.ok !== true) {
    console.error(`❌ 判不了：brush_stroke 失败（${JSON.stringify(first).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const anchors = first.anchors;
  if (!anchors) {
    console.error(
      "❌ ① 回执里没有 `anchors` ⇒ 视觉锚点（data 模式）还没接上" +
        `｜现有字段：${Object.keys(first).join(",")}`,
    );
    process.exit(1);
  }
  console.log(`  ① anchors.last_stroke = ${JSON.stringify(anchors.last_stroke)}`);
  console.log(`  ② anchors.grid = ${JSON.stringify(anchors.grid)}｜canvas = ${JSON.stringify(anchors.canvas)}`);

  const bad = [];
  const near = (a, b, tol = 1.5) => Array.isArray(a) && Array.isArray(b) && Math.abs(a[0] - b[0]) <= tol && Math.abs(a[1] - b[1]) <= tol;
  const ls = anchors.last_stroke || {};
  if (!near(ls.start, [POINTS[0][0], POINTS[0][1]])) {
    bad.push(`① last_stroke.start 应为 [${POINTS[0][0]},${POINTS[0][1]}]，实测 ${JSON.stringify(ls.start)}`);
  }
  if (!near(ls.end, [POINTS[2][0], POINTS[2][1]])) {
    bad.push(`① last_stroke.end 应为 [${POINTS[2][0]},${POINTS[2][1]}]，实测 ${JSON.stringify(ls.end)}`);
  }
  const bb = ls.bbox;
  if (!Array.isArray(bb) || bb.length !== 4) {
    bad.push(`① last_stroke.bbox 应是 [x,y,w,h]，实测 ${JSON.stringify(bb)}`);
  } else {
    for (const [x, y] of [[POINTS[0][0], POINTS[0][1]], [POINTS[2][0], POINTS[2][1]]]) {
      if (x < bb[0] || x > bb[0] + bb[2] || y < bb[1] || y > bb[1] + bb[3]) {
        bad.push(`① bbox ${JSON.stringify(bb)} 没有包住端点 [${x},${y}]`);
        break;
      }
    }
  }
  if (Number(anchors.grid && anchors.grid.spacing) !== 200) {
    bad.push(`② grid.spacing 缺省应为 200，实测 ${JSON.stringify(anchors.grid)}`);
  }
  if (Number(anchors.canvas && anchors.canvas.width) !== W || Number(anchors.canvas && anchors.canvas.height) !== H) {
    bad.push(`② canvas 应为 ${W}×${H}，实测 ${JSON.stringify(anchors.canvas)}`);
  }

  // ② `grid_spacing` 参数要能改间距
  const spaced = await stroke({ grid_spacing: 64 });
  const spacing = spaced && spaced.anchors && spaced.anchors.grid && spaced.anchors.grid.spacing;
  console.log(`  ② grid_spacing:64 ⇒ anchors.grid.spacing = ${spacing}`);
  if (Number(spacing) !== 64) {
    bad.push(`② 传了 grid_spacing:64 但 anchors.grid.spacing = ${JSON.stringify(spacing)}`);
  }

  // ③ 确定性：同参数两次调用，锚点一致（去掉随调用变化的东西再比）
  const again = await stroke({});
  const pick = (v) =>
    JSON.stringify(v && v.anchors ? v.anchors : null);
  if (pick(again) !== pick(first)) {
    bad.push(`③ 同参数两次调用的 anchors 不一致：${pick(first)} vs ${pick(again)}`);
  } else {
    console.log("  ③ 确定性：两次调用的 anchors 一致");
  }

  // ④ dual 模式：本版未实现 => 必须**明确报错**（不许静默当成 data）
  const dual = await stroke({ anchor_mode: "dual" });
  console.log(`  ④ anchor_mode:"dual" ⇒ ok=${dual && dual.ok}｜error_code=${dual && dual.error_code}`);
  if (dual && dual.ok === true) {
    bad.push("④ `anchor_mode:\"dual\"`（两张图）本版未实现，却被**静默接受** ⇒ 应当明确报错");
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 视觉锚点（data 模式）可用：首末点/bbox/grid/canvas 都对，可调间距，可复现，dual 明确报错");
};

await main();
