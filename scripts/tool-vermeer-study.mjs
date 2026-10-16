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

    // ⑤ 虹膜：深棕，用 `color_to` 做渐变 ＋ `color_mixing: pigment` 光谱混色
    //    **★★★ 2026-10-16 修：这一笔此前**每次都失败** ✗ ★★★**（**由新的像素级验收抓到 ✓**）：
    //      **∴ 症状 ✗**：脚本把 `color_mixing` 传给了 **`brush_stroke`** ✗ ——
    //        而那个参数**只有 `medium_stroke` 有** ✓ ⇒ 回执是
    //        「**brush_stroke 不接受参数 color_mixing**」✓ ⇒ **虹膜**一个像素都没画上**✗**。
    //      **∴ 为什么长期没被发现 ✗**：旧验收**只判 `ok`** ✓，而这一笔的失败被记进"缺口清单"后
    //        **仍然 exit 0** ✗ ⇒ **∴ 于是**"虹膜没显出来"✗ 被当成"画质问题"记在台账里 ✓
    //        （§九 甚至去查过"是不是被后画的东西盖住／被 relief 吃掉"✗）★
    //      **∴ 修法 ✗**：**改用 `medium_stroke`（它两个参数都支持 ✓）** ⇒ 正好也是"湿画法"的用法 ✓；
    //        并且把**两个点**改成**同心环填实** ✓ —— 台账 §七 诊断过"2 个相距 0.04×边长的点
    //        去画虹膜 ⇒ 那本来就只是一小笔" ✓ ⇒ **平涂/实心形状要填实** ✓。
    const irisCenter = [SIDE * 0.441, SIDE * 0.4575];
    const irisRadius = SIDE * 0.028;
    const irisPoints = [];
    for (let ring = 1; ring <= 4; ring += 1) {
      const radius = (irisRadius * ring) / 4;
      for (let index = 0; index < 24; index += 1) {
        const angle = (index / 24) * Math.PI * 2;
        irisPoints.push([
          irisCenter[0] + Math.cos(angle) * radius,
          irisCenter[1] + Math.sin(angle) * radius,
          1.0,
        ]);
      }
    }
    const iris = await call(doc, token, "medium_stroke", {
      layer_id: "paint",
      object_id: "iris",
      medium: "oil",
      size: 30,
      load: 0.8,
      wetness: 0.2,
      texture: 0.3,
      color: { r: 92, g: 62, b: 34, a: 255 },
      color_to: { r: 150, g: 118, b: 70, a: 255 },
      color_mixing: "pigment",
      points: irisPoints,
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

    // **★ 主高光（珠光） ✓**（2026-10-16 补 ✓）：**二次反光之外还必须有**一个亮点** ✓**
    //   **∴ 为什么补 ✗**：新的像素级验收量出"珠体 180／最亮 187" ✓ ⇒ **只差 7** ✗
    //     ⇒ **∴ 珍珠看着像**灰塑料球**✗**（台账 §七 记过"珍珠是**软灰团**"✗）★
    //   **∴ 定法 ✗**：珠体是**从暗到亮的渐变**（肌理 ✓），而**点状镜面高光**是**纯几何/硬边** ✓
    //     ⇒ 前者用 `medium_stroke` ✓、后者用 `brush_stroke` 的硬边小笔 ✓（§八 的取舍 ✓）。
    const specular = await call(doc, token, "brush_stroke", {
      layer_id: "paint",
      object_id: "pearl_highlight",
      brush: "100%_Opaque",
      size: 13,
      hardness: 0.95,
      opacity: 1.0,
      color: { r: 253, g: 251, b: 246, a: 255 },
      points: [
        [SIDE * 0.723, SIDE * 0.693],
        [SIDE * 0.728, SIDE * 0.697],
      ],
    });
    steps.push(["珍珠主高光（珠光）", specular && specular.ok === true, specular]);
    if (!specular || specular.ok !== true) note("珍珠的主高光没落上 ⇒ 珍珠会像灰塑料球");

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

  // ---------------------------------------------------------------------------
  // **★ 像素级验收：画出来的到底像不像"眼睛 ＋ 珍珠" ✗ ★**（2026-10-16 ✓）
  //
  // **∴ 为什么非有不可 ✗**：上面那些 `ok === true` **只能证明"工具没报错"** ✓ ——
  //   而验收 v2 抓到的那处硬伤（**虹膜/瞳孔没显出来** ✗，台账 §九 ✓）**恰恰是 `ok` 全绿** ✓
  //   ⇒ **∴ 只判 `ok` 的验收**看不见它**✗**（这正是"静默无效果"那一族 ✓）。
  // **∴ 所以这里直接量像素 ✓**：原始字节本来就在手里 ✓（`render_region {raw:true}` ✓）。
  // **∴ 判据口径 ✗**：**只看**相对亮度关系**与**面积**✓**（**不比对参考图** ✗ ——
  //   "不作弊"那条硬约束就是不许从参考图取像素 ✓）。
  // ---------------------------------------------------------------------------
  const lum = (x, y) => {
    const at = ((y | 0) * width + (x | 0)) * 4;
    return 0.299 * first.bytes[at] + 0.587 * first.bytes[at + 1] + 0.114 * first.bytes[at + 2];
  };
  const at = (fx, fy) => [Math.round(fx * width), Math.round(fy * height)];
  const [scleraX, scleraY] = at(0.37, 0.44);
  // **∴ 采样点必须落在**它自己要量的那个东西**上 ✗**（第 456 轮的教训 ✓）：
  //   **瞳孔画在虹膜**里面** ✓（中心 ≈0.44, 半径 ≈7px ⇒ 跨度 ≈0.431–0.449 ✓）
  //   ⇒ **∴ 虹膜必须取**瞳孔之外**的一点** ✓（0.418 ✓）；两者相差只有 0.001 就会量成同一个东西 ✗。
  const [irisX, irisY] = at(0.418, 0.4575);
  const [pupilX, pupilY] = at(0.44, 0.458);
  const [lightX, lightY] = at(0.454, 0.4415);
  const scleraLum = lum(scleraX, scleraY);
  const irisLum = lum(irisX, irisY);
  const pupilLum = lum(pupilX, pupilY);
  const lightLum = lum(lightX, lightY);
  // **虹膜面积** ✓：眼框里比眼白暗 30 以上的像素数 ⇒ **"一个小点"过不了这一关** ✓
  let irisArea = 0;
  for (let y = Math.round(0.4 * height); y < Math.round(0.52 * height); y += 1) {
    for (let x = Math.round(0.36 * width); x < Math.round(0.54 * width); x += 1) {
      if (lum(x, y) < scleraLum - 30) irisArea += 1;
    }
  }
  // **珍珠**：招牌是"主高光 ＋ 二次反光" ⇒ 高光必须**明显亮于**珠体 ✓
  //   **∴ 判法不能只取一个坐标 ✗**（我第一版就栽在这上面 ✓：取到的两点都在珠体上 ⇒ 187 vs 180 ✗）
  //   ⇒ **∴ 改成"珠体包围盒里**最亮的那一点**"** ✓ —— 那才是"有没有高光"这个问题 ✓
  //     （**∴ 与坐标无关 ✓** ⇒ 编排把高光画在哪儿都算 ✓）。
  const pearlBox = [
    [Math.round(0.65 * width), Math.round(0.64 * height)],
    [Math.round(0.79 * width), Math.round(0.79 * height)],
  ];
  let pearlPeak = 0;
  let pearlPeakAt = [0, 0];
  for (let y = pearlBox[0][1]; y < pearlBox[1][1]; y += 1) {
    for (let x = pearlBox[0][0]; x < pearlBox[1][0]; x += 1) {
      const value = lum(x, y);
      if (value > pearlPeak) {
        pearlPeak = value;
        pearlPeakAt = [x, y];
      }
    }
  }
  const [pearlBodyX, pearlBodyY] = at(0.703, 0.702);
  const pearlLum = pearlPeak;
  const pearlBodyLum = lum(pearlBodyX, pearlBodyY);
  console.log(
    `\n▶ 像素级验收（只看相对关系与面积，不比对参考图 ✓）：\n` +
      `   眼白 ${scleraLum.toFixed(0)} ⇒ 虹膜 ${irisLum.toFixed(0)} ⇒ 瞳孔 ${pupilLum.toFixed(0)}` +
      `｜角膜高光 ${lightLum.toFixed(0)}｜虹膜面积 ${irisArea}px（眼框内比眼白暗 30+ ✓）\n` +
      `   珍珠珠体 ${pearlBodyLum.toFixed(0)}｜珍珠最亮 ${pearlLum.toFixed(0)}（在 ${pearlPeakAt.join(",")} ✓）`,
  );
  const looksWrong = [];
  // **① 虹膜必须真的显出来** ✓（台账 §九 那一格的**判据化** ✓：中心不再是眼白色 ✓）
  if (!(irisLum < scleraLum - 20)) {
    looksWrong.push(`虹膜中心 ${irisLum.toFixed(0)} 不比眼白 ${scleraLum.toFixed(0)} 暗 20 以上 ⇒ 虹膜**没显出来** ✗`);
  }
  // **② 瞳孔必须比虹膜更暗** ✓（"眼睛有没有神"的最低要求 ✓）
  if (!(pupilLum < irisLum - 20)) {
    looksWrong.push(`瞳孔 ${pupilLum.toFixed(0)} 不比虹膜 ${irisLum.toFixed(0)} 暗 20 以上 ⇒ 瞳孔糊在虹膜里 ✗`);
  }
  // **③ 角膜高光必须**明显亮于**瞳孔** ✓（否则"没有神" ✗）
  if (!(lightLum > pupilLum + 60)) {
    looksWrong.push(`角膜高光 ${lightLum.toFixed(0)} 不比瞳孔 ${pupilLum.toFixed(0)} 亮 60 以上 ⇒ 眼睛没神 ✗`);
  }
  // **④ 虹膜不许是"一个小点"** ✓（§七 的诊断 ✓）：面积必须 ≥ 边长的 0.3%（768² ⇒ 1770px 的 0.3% ≈ 1770）
  const irisFloor = Math.round(0.003 * width * height);
  if (!(irisArea >= irisFloor)) {
    looksWrong.push(`虹膜只有 ${irisArea}px < ${irisFloor}px ⇒ 那一笔只是**一个小点** ✗（§七 诊断过同一形态 ✓）`);
  }
  // **⑤ 珍珠必须有高光** ✓（"像塑料球"与"有二次反光"的分界 ✓）
  if (!(pearlLum > pearlBodyLum + 25)) {
    looksWrong.push(`珍珠高光 ${pearlLum.toFixed(0)} 不比珠体 ${pearlBodyLum.toFixed(0)} 亮 25 以上 ⇒ 珍珠会像塑料球 ✗`);
  }

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
  // **★ 像素级验收**不过**就不许报"验收通过" ✗**（2026-10-16 ✓）：
  //   **∴ 为什么放在最后 ✗**：它必须**压过**那句"能画出来" ✓ ——
  //     验收 v2 的教训就是"**能画出来**✓"与"**画出来像不像**✗"是两件事 ✓。
  if (looksWrong.length > 0) {
    console.error(`\n❌ 像素级验收不过（${looksWrong.length} 项）—— 画出来了，但**不像眼睛/珍珠** ✗：`);
    for (const text of looksWrong) console.error(`   · ${text}`);
    console.error("   ⇒ 这是**产品问题**（落笔链路）还是**编排问题**（这一支笔/这个坐标 ✗）？");
    console.error("   ⇒ 台账 §七／§八 的判法：**平涂/实心形状用 `draw_stroke`，肌理/渐变才用物理笔** ✓");
    process.exit(1);
  }
  console.log("✓ 验收：能画出来、逐字节可复现、且**像素级看像眼睛 ＋ 珍珠** ✓");
  process.exit(0);
};

await main();
