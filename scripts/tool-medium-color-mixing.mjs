#!/usr/bin/env node
// **★ 介质路径的 `color_to` / `color_mixing` 判据（P1 缺口修复）★**（第 254 轮）
//
// **缺口来自端到端验收**：`scripts/tool-vermeer-study.mjs` 实测报出
// "`medium_stroke` 不接受参数 `color_mixing`" ✗ —— P2 ⑯ 的光谱混色当时只接进了
// `brush_stroke` 的 `color_to` 渐变，**介质路径（油画/水彩/马克笔/铅笔）画不出渐变** ✗。
//
// **本判据要证三条**：
//   ① 给了 `color_to` ⇒ 画面**真的出现渐变**（近端色 ≠ 远端色）；
//   ② `color_mixing: "pigment"` 与 `"rgb"` 的**结果必须不同**（光谱混色真的接上了 ✓）；
//   ③ **不给 `color_mixing`（缺省）与显式 `"rgb"` 必须逐字节相同** ⇒ "默认就好用、老行为不变" ✓。
//
// **变异（能红的根据）**：把 `medium_point_colors` 的接线改回 `None` ⇒ ① 红（没有渐变）；
// 把 `mixing` 写死成 `Rgb` ⇒ ② 红；把缺省改成 `Pigment` ⇒ ③ 红。
//
// 用法：node scripts/tool-medium-color-mixing.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-medium-color-mixing.mjs <base-url>");
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
const H = 96;

/** 画一笔介质渐变（黄 → 蓝），返回原始 RGBA 与它的指纹。 */
const paint = async (tag, extra) => {
  const doc = `mcm_${tag}_${Date.now().toString(36)}`;
  let created;
  try {
    created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: W, height: H }),
      })
    ).json();
  } catch (error) {
    console.error(`❌ 判不了：连不上（${error}）=> 退出码 3`);
    process.exit(3);
  }
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 => 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  const painted = await call(doc, token, "medium_stroke", {
    layer_id: "L1",
    object_id: "grad",
    medium: "oil",
    size: 40,
    load: 0.6,
    wetness: 0.3,
    // **饱和**的黄 → 蓝：rgb 混出灰、pigment（K/S）混出绿 —— 差别大，才**量得到** ✓。
    //   （第 254 轮实测教训：我先用了一对**低饱和**的暖黄/冷蓝 ⇒ 中段 rgb 与 pigment 只差 ±1 个色阶
    //    ⇒ 经过插件的混色/量化**被抹平** ⇒ 判据②红，而**接线其实是对的** ✗。颜色要选得让差异活下来 ✓。）
    color: { r: 255, g: 255, b: 0, a: 255 },
    color_to: { r: 0, g: 0, b: 255, a: 255 },
    points: [
      [30, H / 2],
      [W - 30, H / 2],
    ],
    ...extra,
  });
  if (!painted || painted.ok !== true) {
    const detail = painted && painted.context && painted.context.detail;
    console.error(
      `❌ 判不了：medium_stroke 失败（${detail ? String(detail).slice(0, 160) : JSON.stringify(painted).slice(0, 120)}）=> 退出码 3`,
    );
    process.exit(3);
  }
  const raw = await call(doc, token, "render_region", {
    region: { x: 0, y: 0, w: W, h: H },
    raw: true,
  });
  const url = raw && (raw.raw_url || raw.rawUrl);
  if (!url) {
    console.error("❌ 判不了：拿不到原始像素 => 退出码 3");
    process.exit(3);
  }
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
  const fingerprint = (() => {
    let hash = 0;
    for (let index = 0; index < bytes.length; index += 4) {
      hash = (hash * 31 + bytes[index] + bytes[index + 1] * 3 + bytes[index + 2] * 7) >>> 0;
    }
    return hash;
  })();
  /** 某一段的平均色（只看有颜料的像素）：用来证"真有渐变"。 */
  const bandColor = (fromX, toX) => {
    let r = 0;
    let g = 0;
    let b = 0;
    let n = 0;
    for (let y = 0; y < H; y += 1) {
      for (let x = fromX; x < toX; x += 1) {
        const at = (y * W + x) * 4;
        if (bytes[at + 3] < 40) continue;
        r += bytes[at];
        g += bytes[at + 1];
        b += bytes[at + 2];
        n += 1;
      }
    }
    return n > 0 ? [r / n, g / n, b / n] : null;
  };
  return { fingerprint, near: bandColor(40, 80), far: bandColor(W - 80, W - 40) };
};

const main = async () => {
  const bad = [];
  const rgb = await paint("rgb", { color_mixing: "rgb" });
  const pigment = await paint("pig", { color_mixing: "pigment" });
  const dflt = await paint("def", {});

  if (!rgb.near || !rgb.far) {
    console.error("❌ 判不了：没量到颜料（可能这一笔没画上）=> 退出码 3");
    process.exit(3);
  }
  const spread = (color) =>
    color ? Math.max(...color) - Math.min(...color) : 0;
  console.log(
    `  近端色 rgb=${rgb.near.map((v) => v.toFixed(0)).join(",")}｜远端色 rgb=${rgb.far.map((v) => v.toFixed(0)).join(",")}`,
  );
  console.log(
    `  近端色 pigment=${pigment.near.map((v) => v.toFixed(0)).join(",")}｜远端色 pigment=${pigment.far.map((v) => v.toFixed(0)).join(",")}`,
  );

  // ① 真有渐变：近端与远端的色必须明显不同
  const gradient = Math.abs(rgb.near[0] - rgb.far[0]) + Math.abs(rgb.near[2] - rgb.far[2]);
  if (!(gradient > 20)) {
    bad.push(`① 没有渐变（近端与远端的 R/B 差只有 ${gradient.toFixed(1)}，要求 >20）⇒ \`color_to\` 没生效`);
  }
  // ② pigment 与 rgb 必须不同
  if (rgb.fingerprint === pigment.fingerprint) {
    bad.push("② `color_mixing` 取 pigment 与 rgb 的画面**逐字节相同** ⇒ 光谱混色没接上");
  }
  // ③ 缺省必须与显式 rgb 逐字节相同
  if (dflt.fingerprint !== rgb.fingerprint) {
    bad.push(
      `③ 不给 \`color_mixing\`（缺省）与显式 "rgb" 不同（${dflt.fingerprint} vs ${rgb.fingerprint}）⇒ 默认行为被改了`,
    );
  }
  // 附：pigment 应当"更偏绿"（黄+蓝混出绿而不是灰）—— 只作为**参考信息**打印，不单独判红
  console.log(
    `  参考：pigment 中段偏绿程度 ${spread(pigment.near).toFixed(1)}｜rgb 中段偏灰程度 ${spread(rgb.near).toFixed(1)}`,
  );

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 介质渐变成立：`color_to` 有渐变、`pigment` 与 `rgb` 不同、**缺省与 rgb 逐字节相同**");
};

await main();
