#!/usr/bin/env node
// **★ 端到端验收 v2：《戴珍珠耳环的少女》的眼睛与珍珠 ★**（第 254 轮）
//
// **需求文档的验收目标**：「让 AI 通过 MCP 画出维梅尔级古典油画，以《戴珍珠耳环的少女》眼睛/珍珠为验证目标」。
//
// **v1 的教训**（`tool-vermeer-study.mjs`）：**工具链 12/12 步全通 ✓，但画得不像** ✗ ——
// 眼形是**多边形**（没用 `smooth` ✓、控制点只给了 7 个 ✗）、虹膜是一个**小点** ✗
// => 那不是"引擎缺东西"，而是**编排差** ✗（架构原则：「引擎做物理、MCP 做编排」✓）。
//
// **v2 改什么（纯编排 ✓，不动引擎一行）**：
//   1. **参数化几何**：杏仁眼 = 上下睑两条**弧**（`smooth: true` ＋ 每弧 16 个控制点 ✓）；
//   2. **虹膜/瞳孔/珍珠 = 实心圆**：用**同心环**（螺旋式来回 ✓）填满，而不是一小笔 ✓；
//   3. **深暖底**：先铺一层暗背景 ⇒ 珍珠与高光才"跳"得出来 ✓（维梅尔的招牌 ✓）；
//   4. **层序**：暗底 → 眼白 → 虹膜（暗到亮渐变）→ 瞳孔 → 高光 → 珍珠（暗到亮）→ 二次反光 →
//      罩染 → 浮雕 → 锚点 → 出图 ✓。
//
// **仍然守纪律** ✓：全程只用 MCP 落笔；**不取任何参考图像素** ✓；整个研究跑**两遍逐字节比对** ✓；
// 退出码 `0` 画出来且可复现／`1` 产品问题／`3` 判不了／`2` 用法错 ✓。
//
// 用法：node scripts/tool-vermeer-study2.mjs <base-url> [--side 768] [--out /tmp/v2.png]
const argv = process.argv.slice(2);
const base = (argv[0] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-vermeer-study2.mjs <base-url> [--side 768] [--out /tmp/v2.png]");
  process.exit(2);
}
const flag = (name, fallback) => {
  const at = argv.indexOf(name);
  return at >= 0 && argv[at + 1] !== undefined ? argv[at + 1] : fallback;
};
const SIDE = Number(flag("--side", 768));
const OUT = flag("--out", "/tmp/vermeer-study2.png");

const gaps = [];
const seen = new Set();
const note = (text) => {
  if (seen.has(text)) return;
  seen.add(text);
  gaps.push(text);
  console.log(`  ⚠️  缺口：${text}`);
};

// **参数化几何**（全部确定性 ✓）
const round2 = (value) => Math.round(value * 100) / 100;
/** 椭圆弧：从 `from` 到 `to`（弧度），`steps` 段 => `steps + 1` 个控制点 ✓。 */
const arc = (cx, cy, rx, ry, from, to, steps) =>
  Array.from({ length: steps + 1 }, (_, index) => {
    const t = from + ((to - from) * index) / steps;
    return [round2(cx + Math.cos(t) * rx), round2(cy + Math.sin(t) * ry)];
  });
/**
 * **实心圆**：用**同心环**填满（从外到内、每环首尾相连 ⇒ 环与环之间来回 ✓）——
 * 这样一笔笔画就能填出实心面 ✓（v1 只画一小笔 ⇒ 那是个点 ✗）。
 */
const ell = (cx, cy, rx, ry, rings, perRing) => {
  const points = [];
  for (let ring = 0; ring < rings; ring += 1) {
    const r = 1 - ring / rings;
    // 每环从"上一环的终点"附近接上 ⇒ 路径连续 ✓
    const start = ring % 2 === 0 ? Math.PI * 0.5 : Math.PI * 1.5;
    const direction = ring % 2 === 0 ? 1 : -1;
    for (let step = 0; step <= perRing; step += 1) {
      const t = start + (direction * Math.PI * 2 * step) / perRing;
      points.push([round2(cx + Math.cos(t) * rx * r), round2(cy + Math.sin(t) * ry * r)]);
    }
  }
  return points;
};

const main = async () => {
  const call = async (doc, token, tool, args) =>
    (
      await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(args || {}),
      })
    ).json();

  const newDoc = async (tag) => {
    const doc = `v2_${tag}_${Date.now().toString(36)}`;
    let created;
    try {
      created = await (
        await fetch(`${base}/api/documents`, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ doc_id: doc, width: SIDE, height: SIDE }),
        })
      ).json();
    } catch (error) {
      console.error(`❌ 判不了：连不上服务（${error}）=> 退出码 3`);
      process.exit(3);
    }
    if (!created || !created.token) {
      console.error("❌ 判不了：建不了文档 => 退出码 3");
      process.exit(3);
    }
    return [doc, created.token];
  };

  const paint = async (tag) => {
    const [doc, token] = await newDoc(tag);
    const steps = [];
    const S = SIDE;
    // 眼睛与珍珠的位置（画布比例 ✓）
    const eye = { cx: S * 0.38, cy: S * 0.42, rx: S * 0.15, ry: S * 0.075 };
    const iris = { cx: S * 0.365, cy: S * 0.428, r: S * 0.055 };
    const pearl = { cx: S * 0.72, cy: S * 0.72, r: S * 0.085 };

    await call(doc, token, "texture_background", { texture: "Paper001.png", mode: "tile" });

    // ── 0. **深暖底**（维梅尔的暗背景 ⇒ 珍珠才跳得出来 ✓）────────────────────
    await call(doc, token, "create_layer", { layer_id: "ground", name: "ground" });
    // **★ 平涂要用 `draw_stroke`（纯几何、实心）★**：v2c 的教训 —— 用 `medium_stroke` 铺底
    //   得到的是**一串圆点** ✗（介质只在控制点落 dab ✓）=> 平涂/实心形状该用几何笔迹 ✓。
    // **★ 铺底改用 `fill_region`（实心矩形）★**（2026-10-15 ✓）：原来用 `draw_stroke`
    //   三笔宽笔 ✓ ⇒ 每笔两头是**圆头笔帽** ✗ ⇒ 画面上留下**花瓣状叠痕** ✗（第 68 轮记下的硬伤 ✓）。
    //   平铺本来就该用"填充" ✓ ⇒ 一次调用、**无边角** ✓、也更快 ✓。
    //   （参数是上一轮试出来的 ✓：`{layer_id, shape:{kind:"rect",x,y,w,h}, color, opacity}` ✓。）
    const ground = await call(doc, token, "fill_region", {
      layer_id: "ground",
      shape: { kind: "rect", x: 0, y: 0, w: S, h: S },
      opacity: 1.0,
      color: { r: 30, g: 25, b: 22, a: 255 },
    });
    steps.push(["深暖底（暗背景）", ground && ground.ok === true, ground]);

    // ── 1. 眼白（巩膜）────────────────────────────────────────────────────
    await call(doc, token, "create_layer", { layer_id: "skin", name: "skin" });
    // 眼白也是**实心几何**（`hardness: 1` ⇒ 边干净 ✓）；用椭圆填充 ✓（v2b 之前是圆 ✗）。
    const sclera = await call(doc, token, "draw_stroke", {
      layer_id: "skin",
      object_id: "sclera",
      data: {
        points: ell(eye.cx, eye.cy, eye.rx * 0.94, eye.ry * 0.98, 3, 24),
        size: S * 0.05,
        hardness: 1.0,
        opacity: 1.0,
        color: { r: 216, g: 208, b: 196, a: 255 },
      },
    });
    steps.push(["眼白（实心椭圆）", sclera && sclera.ok === true, sclera]);

    // ── 2. 虹膜：**实心圆** ＋ 暗到亮渐变（维梅尔的虹膜上暗下亮 ✓）────────────
    const irisPaint = await call(doc, token, "medium_stroke", {
      layer_id: "skin",
      object_id: "iris",
      medium: "oil",
      size: S * 0.035,
      load: 0.7,
      wetness: 0.25,
      texture: 0.45,
      color: { r: 58, g: 44, b: 30, a: 255 },
      color_to: { r: 150, g: 116, b: 68, a: 255 },
      color_mixing: "pigment",
      points: ell(iris.cx, iris.cy, iris.r, iris.r * 1.12, 6, 26),
    });
    steps.push(["虹膜（实心圆 ＋ 光谱渐变）", irisPaint && irisPaint.ok === true, irisPaint]);
    if (!irisPaint || irisPaint.ok !== true) note("虹膜那笔失败（`color_to`/`color_mixing` 走不通）");

    // ── 3. 瞳孔（更小的实心圆，近黑）──────────────────────────────────────
    const pupil = await call(doc, token, "medium_stroke", {
      layer_id: "skin",
      object_id: "pupil",
      medium: "oil",
      size: S * 0.022,
      load: 0.8,
      wetness: 0.2,
      texture: 0.2,
      color: { r: 14, g: 11, b: 10, a: 255 },
      points: ell(iris.cx, iris.cy, iris.r * 0.45, iris.r * 0.5, 4, 18),
    });
    steps.push(["瞳孔（实心圆）", pupil && pupil.ok === true, pupil]);

    // ── 4. 上下睑（**弧** ＋ smooth ⇒ 杏仁眼）──────────────────────────────
    const upper = await call(doc, token, "brush_stroke", {
      layer_id: "skin",
      object_id: "upper_lid",
      brush: "classic-pen.myb",
      size: S * 0.008,
      color: { r: 38, g: 28, b: 24, a: 255 },
      points: arc(eye.cx, eye.cy, eye.rx, eye.ry, Math.PI * 1.05, Math.PI * 1.95, 16),
      smooth: true,
      humanize: true,
    });
    const lower = await call(doc, token, "brush_stroke", {
      layer_id: "skin",
      object_id: "lower_lid",
      brush: "classic-pen.myb",
      size: S * 0.005,
      color: { r: 62, g: 48, b: 42, a: 255 },
      points: arc(eye.cx, eye.cy, eye.rx * 0.96, eye.ry * 0.96, Math.PI * 0.08, Math.PI * 0.92, 14),
      smooth: true,
      humanize: true,
    });
    steps.push(["上睑弧（smooth）", upper && upper.ok === true, upper]);
    steps.push(["下睑弧（smooth）", lower && lower.ok === true, lower]);

    // ── 5. 角膜高光（点睛：小、硬、亮 ✓）──────────────────────────────────
    const catchlight = await call(doc, token, "brush_stroke", {
      layer_id: "skin",
      object_id: "catchlight",
      brush: "100%_Opaque",
      size: S * 0.016,
      hardness: 0.85,
      color: { r: 255, g: 253, b: 246, a: 255 },
      points: [
        [round2(iris.cx - iris.r * 0.38), round2(iris.cy - iris.r * 0.42)],
        [round2(iris.cx - iris.r * 0.28), round2(iris.cy - iris.r * 0.34)],
      ],
    });
    steps.push(["角膜高光（点睛）", catchlight && catchlight.ok === true, catchlight]);
    if (!catchlight || catchlight.ok !== true) note("高光那笔失败 ⇒ 眼睛会「没有神」");

    // ── 6. 珍珠：**实心圆** ＋ 暗到亮渐变（左上亮、右下暗 ⇒ 球感 ✓）──────────
    const pearlBody = await call(doc, token, "medium_stroke", {
      layer_id: "skin",
      object_id: "pearl",
      medium: "oil",
      size: S * 0.045,
      load: 0.55,
      wetness: 0.35,
      texture: 0.12,
      color: { r: 74, g: 70, b: 66, a: 255 },
      color_to: { r: 246, g: 242, b: 232, a: 255 },
      color_mixing: "pigment",
      points: ell(pearl.cx, pearl.cy, pearl.r, pearl.r, 8, 30),
    });
    steps.push(["珍珠本体（实心圆 ＋ 光谱渐变）", pearlBody && pearlBody.ok === true, pearlBody]);

    // 珍珠的**二次反光**（下方一点冷灰 —— 这是珍珠的招牌 ✓）
    const reflected = await call(doc, token, "brush_stroke", {
      layer_id: "skin",
      object_id: "pearl_reflected",
      brush: "100%_Opaque",
      size: S * 0.022,
      hardness: 0.5,
      opacity: 0.7,
      color: { r: 186, g: 178, b: 164, a: 255 },
      points: ell(pearl.cx - pearl.r * 0.05, pearl.cy + pearl.r * 0.45, pearl.r * 0.34, pearl.r * 0.34, 3, 16),
    });
    steps.push(["珍珠二次反光", reflected && reflected.ok === true, reflected]);

    // 珍珠主高光（左上，小而硬 ✓）
    const pearlSpec = await call(doc, token, "brush_stroke", {
      layer_id: "skin",
      object_id: "pearl_specular",
      brush: "100%_Opaque",
      size: S * 0.013,
      hardness: 0.9,
      color: { r: 255, g: 254, b: 250, a: 255 },
      anchor_mode: "data",
      points: [
        [round2(pearl.cx - pearl.r * 0.42), round2(pearl.cy - pearl.r * 0.46)],
        [round2(pearl.cx - pearl.r * 0.30), round2(pearl.cy - pearl.r * 0.36)],
      ],
    });
    steps.push(["珍珠主高光（带锚点）", pearlSpec && pearlSpec.ok === true, pearlSpec]);
    if (!pearlSpec || !pearlSpec.anchors) note("`brush_stroke{anchor_mode:\"data\"}` 没有回结构化锚点");

    // ── 7. 罩染（暗部统一）＋ 浮雕（厚涂）──────────────────────────────────
    const glaze = await call(doc, token, "brush_stroke", {
      layer_id: "skin",
      object_id: "glaze_shadow",
      brush: "100%_Opaque",
      size: S * 0.18,
      opacity: 0.1,
      glaze: true,
      glaze_passes: 4,
      color: { r: 74, g: 56, b: 38, a: 255 },
      points: [
        [S * 0.28, S * 0.56],
        [S * 0.5, S * 0.62],
      ],
    });
    steps.push(["罩染（glaze 4 遍）", glaze && glaze.ok === true, glaze]);

    const relief = await call(doc, token, "add_filter", {
      layer_id: "skin",
      filter_name: "relief",
      params: { azimuth: 135, elevation: 45, strength: 1.4, height_gain: 22 },
    });
    steps.push(["厚涂浮雕（relief 1.4）", relief && relief.ok === true, relief]);
    if (!relief || relief.ok !== true) note("浮雕滤镜没加上 ⇒ P2 ⑮ 在这条链路上没走通");

    const rendered = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: S, h: S },
      raw: true,
    });
    const url = rendered && (rendered.raw_url || rendered.rawUrl);
    if (!url) {
      console.error("❌ 判不了：render_region 没给出 raw_url => 退出码 3");
      process.exit(3);
    }
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());

    for (const [name, ok, reply] of steps) {
      if (!ok) {
        const detail = reply && reply.context && reply.context.detail;
        console.log(`     ↳ ${name} 的错误原文：${detail ? String(detail).slice(0, 140) : "（无）"}`);
      }
    }
    const failed = steps.filter(([, ok]) => !ok);
    console.log(
      `  [${tag}] 步骤 ${steps.length - failed.length}/${steps.length} 成功${failed.length ? `｜失败：${failed.map(([n]) => n).join("、")}` : ""}`,
    );
    return { bytes, rendered };
  };

  console.log(`▶ 验收 v2：眼睛 ＋ 珍珠（${SIDE}×${SIDE}，参数化几何 ＋ smooth，全程只用 MCP）`);
  const first = await paint("a");
  const second = await paint("b");
  const same =
    first.bytes.length === second.bytes.length &&
    first.bytes.every((value, index) => value === second.bytes[index]);
  console.log(`\n▶ 确定性：两遍逐字节比对 ⇒ ${same ? "**一致** ✓" : "**不一致** ✗"}`);

  const { deflateSync } = await import("node:zlib");
  const { writeFileSync } = await import("node:fs");
  const width = Number(first.rendered.width || SIDE);
  const height = Number(first.rendered.height || SIDE);
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y += 1) {
    raw[y * (width * 4 + 1)] = 0;
    Buffer.from(first.bytes.buffer, first.bytes.byteOffset + y * width * 4, width * 4).copy(
      raw,
      y * (width * 4 + 1) + 1,
    );
  }
  const crcTable = [];
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
  }
  const crc32 = (buffer) => {
    let c = 0xffffffff;
    for (const byte of buffer) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const head = Buffer.alloc(8);
    head.writeUInt32BE(data.length, 0);
    head.write(type, 4, "ascii");
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0);
    return Buffer.concat([head, data, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  writeFileSync(
    OUT,
    Buffer.concat([
      Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      chunk("IHDR", ihdr),
      chunk("IDAT", deflateSync(raw)),
      chunk("IEND", Buffer.alloc(0)),
    ]),
  );
  console.log(`▶ 出图：${OUT}（${width}×${height}）`);
  console.log(`\n▶ 缺口清单（${gaps.length} 条）：`);
  if (gaps.length === 0) console.log("  （无）—— 这条链路没有暴露缺口");
  for (const [index, text] of gaps.entries()) console.log(`  ${index + 1}. ${text}`);

  if (!same) {
    console.error("❌ 两遍结果不一致 ⇒ 违反「确定性」硬约束（产品问题）");
    process.exit(1);
  }
  console.log("✓ 验收 v2：画出来了、且逐字节可复现（像不像仍需人眼 ✓）");
};

await main();
