#!/usr/bin/env node
// **★ 厚涂浮雕 relief 的端到端判据（P2 #15）★**（第 254 轮）
//
// **需求 P2 #15**："高度场（厚涂浮雕：法向 + 光源的明度偏置）"。
//
// **入口（本轮查清）**：滤镜是 **Filter 类型对象**（`render.rs:2736` 读 `data.filter_name`
// 与 `data.params`）=> 用户路径就是 **`add_filter`**（spec 在 `tools.rs:2471`）
// => `add_filter{layer_id, filter_name:"relief", params:{…}}`。
//
// **判据要证的（与数据层判据同一条物理，但走完整链路）**：
//   ① 同一块"厚涂堆"（软边 dab => alpha 有高差），**光从右（az=0）** => **右坡（迎光）更亮**；
//   ② **光从左（az=180）** => **反号**（右坡更暗）。
//      这两条合起来证明"**法向 + 光源**"真的在渲染链路里工作，而不是一个无方向的提亮滤镜。
//   ③ 不给滤镜 => 两侧亮度差接近 0（没有方向性的对照）；
//   ④ 确定性：同参数两次画面**逐字节一致**。
//
// **变异（能红的根据）**：把 `relief` 从 `FilterKind::from_name` 里删掉 => ①② 红（滤镜被忽略）；
// 把法向符号写反 => ① 与 ② 同时反号 => 红；把 `strength` 忽略（恒定提亮）=> ① 红。
//
// 用法：node scripts/tool-relief.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-relief.mjs <base-url>");
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

const W = 160;
const H = 96;

/** 造一块"厚涂堆"：软边（hardness 0）小笔尖点一下 => alpha 呈钟形。 */
const build = async (tag, filterParams) => {
  const doc = `rf_${tag}_${Date.now().toString(36)}`;
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
  // 软边、单点 => 一块钟形的"颜料堆"（alpha 有高差，正是浮雕要的高度场）
  const painted = await call(doc, token, "brush_stroke", {
    layer_id: "L1",
    object_id: "blob",
    brush: "100%_Opaque",
    size: 64,
    // **实测**：`hardness: 0.0` 这一笔会**零沉积**（引擎直接按"没落下任何像素"拒绝）；
    // `0.5` 或省略都能画上。软边的高差是浮雕的前提 => 用 0.5。
    hardness: 0.5,
    opacity: 1.0,
    color: { r: 128, g: 128, b: 128, a: 255 },
    // **两点**（单点会被引擎按"路径总长 0px、撑不满 dab 间距"拒绝 —— 实测踩到），
    // 并且**竖直**摆放：软边（hardness 0）=> alpha 在**横向**有平滑高差，
    // 这样"左坡/右坡"才有意义（横放的话左右两带都在中段、高度相同）。
    points: [[W / 2, H / 2 - 40, 1.0], [W / 2, H / 2 + 40, 1.0]],
  });
  if (!painted || painted.ok !== true) {
    console.error(`❌ 判不了：画"厚涂堆"失败（${JSON.stringify(painted).slice(0, 140)}）⇒ 退出码 3`);
    process.exit(3);
  }
  if (filterParams) {
    const added = await call(doc, token, "add_filter", {
      layer_id: "L1",
      filter_name: "relief",
      params: filterParams,
    });
    if (!added || added.ok !== true) {
      console.error(
        `❌ 判不了：add_filter 失败（${JSON.stringify(added).slice(0, 160)}）⇒ 退出码 3`,
      );
      process.exit(3);
    }
  }
  const raw = await call(doc, token, "render_region", {
    region: { x: 0, y: 0, w: W, h: H },
    raw: true,
  });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) {
    console.error("❌ 判不了：量不到渲染结果 ⇒ 退出码 3");
    process.exit(3);
  }
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
  const rw = Number(raw.width || W);
  const rh = Number(raw.height || H);
  if (rw * rh * 4 !== bytes.length) {
    console.error("❌ 判不了：像素尺寸不符 ⇒ 退出码 3");
    process.exit(3);
  }
  // 左右两带的平均亮度（只看有颜料、alpha 足够高的像素）
  const band = (left) => {
    let sum = 0;
    let n = 0;
    for (let y = 0; y < rh; y += 1) {
      for (let x = 0; x < rw; x += 1) {
        const inBand = left ? x >= rw / 4 && x < rw / 2 : x >= rw / 2 && x < (rw * 3) / 4;
        if (!inBand) continue;
        const at = (y * rw + x) * 4;
        if (bytes[at + 3] < 40) continue;
        sum += (bytes[at] + bytes[at + 1] + bytes[at + 2]) / 3;
        n += 1;
      }
    }
    return n > 0 ? sum / n : null;
  };
  let fingerprint = 0;
  for (let i = 0; i < bytes.length; i += 4) {
    fingerprint = (fingerprint * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
  }
  return { left: band(true), right: band(false), fingerprint };
};

const main = async () => {
  const bad = [];
  const plain = await build("plain", null);
  const fromRight = await build("right", { azimuth: 0, elevation: 45, strength: 0.8, height_gain: 30 });
  const fromLeft = await build("left", { azimuth: 180, elevation: 45, strength: 0.8, height_gain: 30 });
  const again = await build("again", { azimuth: 0, elevation: 45, strength: 0.8, height_gain: 30 });

  if (plain.left === null || plain.right === null || fromRight.left === null || fromRight.right === null || fromLeft.left === null || fromLeft.right === null) {
    console.error("❌ 判不了：有一条链路没量到（可能没画上或滤镜把图层清空了）⇒ 退出码 3");
    process.exit(3);
  }
  const spread = (v) => (v.left === null || v.right === null ? null : v.right - v.left);
  const plainDiff = spread(plain);
  const rightDiff = spread(fromRight);
  const leftDiff = spread(fromLeft);
  console.log(
    `  右坡−左坡：无滤镜 ${plainDiff.toFixed(2)}｜光从右(az=0) ${rightDiff.toFixed(2)}｜光从左(az=180) ${leftDiff.toFixed(2)}`,
  );

  // ③ 对照：不给滤镜时不该有方向性
  if (Math.abs(plainDiff) > 2.0) {
    bad.push(`③ 无滤镜时两侧已经有亮度差（${plainDiff.toFixed(2)}，要求 |·|≤2）⇒ 前提不成立`);
  }
  // ① 光从右 => 右坡更亮
  if (!(rightDiff > 0)) {
    bad.push(`① 光从右侧来时右坡没有更亮（${rightDiff.toFixed(2)}）⇒ 浮雕没生效或法向反了`);
  }
  // ② 光从左 => 反号
  if (!(leftDiff < 0)) {
    bad.push(`② 光从左侧来时右坡没有变暗（${leftDiff.toFixed(2)}）⇒ 光源方向没被用上`);
  }
  if (!(rightDiff * leftDiff < 0)) {
    bad.push(`② 换光源方向没有让明暗**反号**（右光 ${rightDiff.toFixed(2)}／左光 ${leftDiff.toFixed(2)}）`);
  }
  // ④ 确定性（同参数两次逐字节）
  if (again.fingerprint !== fromRight.fingerprint) {
    bad.push(`④ 同参数两次画面不同（${fromRight.fingerprint} vs ${again.fingerprint}）⇒ 不满足确定性`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 浮雕端到端成立：add_filter{filter_name:"relief"} 让"迎光坡更亮、背光坡更暗"（右光 ${rightDiff.toFixed(2)}／左光 ${leftDiff.toFixed(2)}，对照 ${plainDiff.toFixed(2)}），且可复现`,
  );
};

await main();
