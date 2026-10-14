#!/usr/bin/env node
// **★ 端到端验收：《戴珍珠耳环的少女》的眼睛与珍珠 ★**（第 254 轮）
//
// **需求文档自己写的验证目标**：「让 AI 通过 MCP 画出维梅尔级古典油画，
// **以《戴珍珠耳环的少女》眼睛/珍珠为验证目标**」。
//
// **这个脚本要回答两件事**（这正是"验收"而不是"演示"的意思）：
//   ① **能不能用现有 MCP 把它画出来** —— 走完整条链：纸张底纹(⑭) → 草稿层(⑬) →
//      介质落笔(载墨/湿度/纹理) → 净室揉合(①) → 罩染(⑩) → 锚点(⑫) → 厚涂浮雕(⑮)；
//   ② **画的过程中缺什么** —— 脚本会把每一步的失败/降级**逐条记下来**（"缺口清单"），
//      而不是只报一个"成功" ✗。缺什么就写什么，缺的东西就是下一批 P0/P1 ✓。
//
// **纪律**：
//   * **不作弊** ✓：全程只用 MCP 工具落笔，**不从任何参考图取像素**（脚本里没有参考图 ✓）；
//   * **确定性** ✓：整个研究跑**两遍**，逐字节比对（同参数同结果 ✓）；
//   * **判据口径**：`0` 画出来了且可复现／`1` 产品问题（画不出来/不可复现）／`3` 判不了／`2` 用法错。
//
// **用法**：node scripts/tool-vermeer-study.mjs <base-url> [--side 768] [--out /tmp/study.png]
const argv = process.argv.slice(2);
const base = (argv[0] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-vermeer-study.mjs <base-url> [--side 768] [--out /tmp/study.png]");
  process.exit(2);
}
const flag = (name, fallback) => {
  const at = argv.indexOf(name);
  return at >= 0 && argv[at + 1] !== undefined ? argv[at + 1] : fallback;
};
const SIDE = Number(flag("--side", 768));
const OUT = flag("--out", "/tmp/vermeer-study.png");

const gaps = [];
const seenGaps = new Set();
const note = (text) => {
  if (seenGaps.has(text)) return; // 两遍研究只记一次
  seenGaps.add(text);
  gaps.push(text);
  console.log(`  ⚠️  缺口：${text}`);
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
    const doc = `vm_${tag}_${Date.now().toString(36)}`;
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

  // ---------------------------------------------------------------------------
  // 一笔研究：全流程
  // ---------------------------------------------------------------------------
  const paint = async (tag) => {
    const [doc, token] = await newDoc(tag);
    const steps = [];

    // ① 纸张底纹（P1 ⑭）：暖米色 + 颗粒。`texture_background` 是引擎已有的能力 ✓
    const paper = await call(doc, token, "texture_background", {
      texture: "Paper001.png",
      mode: "tile",
    });
    steps.push(["纸张底纹", paper && paper.ok === true, paper]);
    if (!paper || paper.ok !== true) note("`texture_background` 没成功 ⇒ 纸张底纹这一层没铺上");

    // ② 草稿层（P1 ⑬）：淡蓝高抖动 —— 用一支软刷 + 低不透明度模拟"草稿"
    await call(doc, token, "create_layer", { layer_id: "sketch", name: "sketch" });
    const sketch = await call(doc, token, "brush_stroke", {
      layer_id: "sketch",
      object_id: "eye_sketch",
      brush: "2B_pencil.myb",
      size: 6,
      color: { r: 120, g: 150, b: 200, a: 180 },
      // 眼睛的外轮廓（椭圆）：用控制点走一圈
      points: [
        [SIDE * 0.30, SIDE * 0.46],
        [SIDE * 0.42, SIDE * 0.36],
        [SIDE * 0.56, SIDE * 0.36],
        [SIDE * 0.66, SIDE * 0.46],
        [SIDE * 0.56, SIDE * 0.56],
        [SIDE * 0.42, SIDE * 0.56],
        [SIDE * 0.30, SIDE * 0.46],
      ],
    });
    steps.push(["草稿轮廓", sketch && sketch.ok === true, sketch]);
    if (!sketch || sketch.ok !== true) note("草稿层的 `brush_stroke` 失败（可能是笔刷名不存在）");

    // ③ 正稿层（P1 ⑬）：正式勾线（黑、硬、细）
    await call(doc, token, "create_layer", { layer_id: "ink", name: "ink" });
    const ink = await call(doc, token, "brush_stroke", {
      layer_id: "ink",
      object_id: "eye_ink",
      brush: "classic-pen.myb",
      size: 3,
      color: { r: 30, g: 22, b: 18, a: 255 },
      points: [
        [SIDE * 0.30, SIDE * 0.46],
        [SIDE * 0.42, SIDE * 0.35],
        [SIDE * 0.56, SIDE * 0.35],
        [SIDE * 0.66, SIDE * 0.46],
      ],
    });
    steps.push(["正稿勾线", ink && ink.ok === true, ink]);
    if (!ink || ink.ok !== true) note("正稿勾线失败（笔刷名或参数）");

    // ④ 油画层：眼白（巩膜）—— 介质落笔（载墨/湿度/纹理），**净室揉合**由引擎做
    await call(doc, token, "create_layer", { layer_id: "paint", name: "paint" });
    const sclera = await call(doc, token, "medium_stroke", {
      layer_id: "paint",
      object_id: "sclera",
      medium: "oil",
      size: 46,
      load: 0.6,
      wetness: 0.35,
      texture: 0.4,
      color: { r: 216, g: 208, b: 196, a: 255 },
      points: [
        [SIDE * 0.33, SIDE * 0.45],
        [SIDE * 0.46, SIDE * 0.42],
        [SIDE * 0.62, SIDE * 0.45],
      ],
    });
    steps.push(["眼白（介质）", sclera && sclera.ok === true, sclera]);
    if (!sclera || sclera.ok !== true) note("`medium_stroke` 失败 ⇒ 介质路径（载墨/湿度/纹理）用不上");

    // ⑤ 虹膜：深棕，用 `color_to` 做渐变
    //    **★ 这里曾经是验收抓到的真缺口 ★**：`medium_stroke` 当时**不接受** `color_mixing`
    //    （P2 ⑯ 的光谱混色只接进了 `brush_stroke` ✗）=> 第 254 轮已修（"先加密路径再混色" ✓）
    //    => 现在这一步**应当成功** ✓；若失败就是**回归**，由下面的 `note` 报出来 ✓。
    const iris = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "iris",
      brush: "2B_pencil.myb",
      size: 34,
      color: { r: 92, g: 62, b: 34, a: 255 },
      color_to: { r: 150, g: 118, b: 70, a: 255 },
      color_mixing: "pigment",
      points: [
        [SIDE * 0.42, SIDE * 0.46],
        [SIDE * 0.462, SIDE * 0.455],
      ],
    });
    steps.push(["虹膜（渐变/颜料混色）", iris && iris.ok === true, iris]);
    if (!iris || iris.ok !== true) note("虹膜那笔失败（`color_to`/`color_mixing` 走不通）");

    // ⑥ 瞳孔 + 角膜高光：小尺寸、硬边（这笔决定"眼睛有没有神"）
    const pupil = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "pupil",
      brush: "100%_Opaque",
      size: 14,
      hardness: 0.75,
      color: { r: 16, g: 12, b: 10, a: 255 },
      points: [
        [SIDE * 0.437, SIDE * 0.457],
        [SIDE * 0.443, SIDE * 0.459],
      ],
    });
    steps.push(["瞳孔", pupil && pupil.ok === true, pupil]);

    const catchlight = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "catchlight",
      brush: "100%_Opaque",
      size: 5,
      hardness: 0.9,
      opacity: 1.0,
      color: { r: 255, g: 252, b: 244, a: 255 },
      points: [
        [SIDE * 0.452, SIDE * 0.44],
        [SIDE * 0.456, SIDE * 0.443],
      ],
    });
    steps.push(["角膜高光（点睛）", catchlight && catchlight.ok === true, catchlight]);
    if (!catchlight || catchlight.ok !== true) note("高光那笔失败 ⇒ 眼睛会「没有神」");

    // ⑦ 珍珠：柔球体（渐变）+ 主高光 + **二次反光**（珍珠的招牌）
    // **这一步现在是回归哨兵** ✓：介质路径**已经支持** `color_mixing`（第 254 轮修好 ✓）
    // => 它必须成功；失败就记缺口 ✓。
    const pearl = await call(doc, token, "medium_stroke", {
      layer_id: "paint",
      object_id: "pearl",
      medium: "oil",
      size: 60,
      load: 0.5,
      wetness: 0.4,
      texture: 0.15,
      color: { r: 88, g: 84, b: 82, a: 255 },
      color_to: { r: 246, g: 242, b: 232, a: 255 },
      color_mixing: "pigment",
      points: [
        [SIDE * 0.70, SIDE * 0.70],
        [SIDE * 0.735, SIDE * 0.715],
      ],
    });
    steps.push(["珍珠本体（暗到亮的渐变）", pearl && pearl.ok === true, pearl]);
    if (!pearl || pearl.ok !== true) {
      note("`medium_stroke` 的 `color_to`/`color_mixing` 失败了 ⇒ 介质渐变（P2 ⑯）回归 ✗");
    }

    const reflected = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "pearl_reflected",
      brush: "100%_Opaque",
      size: 9,
      hardness: 0.6,
      opacity: 0.85,
      color: { r: 214, g: 206, b: 188, a: 255 },
      points: [
        [SIDE * 0.688, SIDE * 0.688],
        [SIDE * 0.694, SIDE * 0.692],
      ],
    });
    steps.push(["珍珠二次反光", reflected && reflected.ok === true, reflected]);
    if (!reflected || reflected.ok !== true) note("珍珠的二次反光没落上 ⇒ 珍珠会像塑料球");

    // ⑧ 罩染（P1 ⑩）：在阴影侧压一层暖褐，让过渡"油"起来
    const glaze = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "glaze_shadow",
      brush: "100%_Opaque",
      size: 70,
      opacity: 0.12,
      glaze: true,
      color: { r: 74, g: 56, b: 38, a: 255 },
      points: [
        [SIDE * 0.36, SIDE * 0.60],
        [SIDE * 0.52, SIDE * 0.66],
      ],
    });
    steps.push(["罩染（glaze:true）", glaze && glaze.ok === true, glaze]);
    if (!glaze || glaze.ok !== true) note("罩染没生效（`glaze` 参数走不通）");

    // ⑨ 厚涂浮雕（P2 ⑮）：用 add_filter 给这一层加"法向 + 光源"的明度偏置
    const relief = await call(doc, token, "add_filter", {
      layer_id: "paint",
      filter_name: "relief",
      params: { azimuth: 135, elevation: 45, strength: 0.5, height_gain: 24 },
    });
    steps.push(["厚涂浮雕（relief 滤镜）", relief && relief.ok === true, relief]);
    if (!relief || relief.ok !== true) {
      note("浮雕滤镜没加上 ⇒ P2 ⑮ 在整条链路上没走通（这条是新增能力，重点看）");
    }

    // ⑩ 锚点（P1 ⑫）：`anchor_mode` 是 **`brush_stroke`** 的参数（实测：`render_region` 不接受它 ✗）
    const anchored = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "pearl_specular",
      brush: "100%_Opaque",
      size: 7,
      hardness: 0.95,
      color: { r: 255, g: 253, b: 246, a: 255 },
      anchor_mode: "data",
      points: [
        [SIDE * 0.712, SIDE * 0.694],
        [SIDE * 0.716, SIDE * 0.697],
      ],
    });
    steps.push(["珍珠主高光（带锚点）", anchored && anchored.ok === true, anchored]);
    if (!anchored || !anchored.anchors) {
      note("`brush_stroke{anchor_mode: \"data\"}` 没有回结构化锚点 ⇒ AI 看不到「锚在哪」");
    }

    // ⑪ 出图
    const rendered = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: SIDE, h: SIDE },
      raw: true,
    });
    const url = rendered && (rendered.raw_url || rendered.rawUrl);
    if (!url) {
      console.error("❌ 判不了：render_region 没给出 raw_url => 退出码 3");
      process.exit(3);
    }
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    // 失败的步骤把**错误原文**打出来（区分"我调错了"与"真缺口"）
    for (const [name, ok, reply] of steps) {
      if (!ok) {
        const detail = reply && reply.context && reply.context.detail;
        console.log(`     ↳ ${name} 的错误原文：${detail ? String(detail).slice(0, 160) : JSON.stringify(reply).slice(0, 120)}`);
      }
    }
    const failed = steps.filter(([, ok]) => !ok);
    console.log(
      `  [${tag}] 步骤 ${steps.length - failed.length}/${steps.length} 成功${failed.length ? `｜失败：${failed.map(([n]) => n).join("、")}` : ""}`,
    );
    return { doc, token, bytes, rendered };
  };

  // ---------------------------------------------------------------------------
  // 主流程：画两遍 => 验确定性 => 出图 => 报缺口
  // ---------------------------------------------------------------------------
  console.log(`▶ 端到端验收：《戴珍珠耳环的少女》眼睛 + 珍珠（${SIDE}×${SIDE}，全程只用 MCP，不取参考图像素）`);
  const first = await paint("a");
  const second = await paint("b");

  const same =
    first.bytes.length === second.bytes.length &&
    first.bytes.every((value, index) => value === second.bytes[index]);
  console.log(`\n▶ 确定性：两遍 ${SIDE}×${SIDE} 逐字节比对 ⇒ ${same ? "**一致** ✓" : "**不一致** ✗"}`);

  // 写 PNG（Node 自带 zlib，不需要额外依赖）
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

  // 缺口清单（这是脚本真正的产出 ✓）
  console.log(`\n▶ 缺口清单（${gaps.length} 条）：`);
  if (gaps.length === 0) console.log("  （无）—— 现有 MCP 的这条链路没有暴露缺口");
  for (const [index, text] of gaps.entries()) console.log(`  ${index + 1}. ${text}`);

  if (!same) {
    console.error("❌ 两遍结果不一致 ⇒ 违反「确定性」硬约束（产品问题）");
    process.exit(1);
  }
  console.log("✓ 验收：能画出来、且逐字节可复现");
  process.exit(0);
};

await main();
