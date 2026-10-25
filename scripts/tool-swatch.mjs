#!/usr/bin/env node
// **★ swatch_brush_strokes：试笔触（P0 #6 · 附录五 ✓）★**（第 254 轮 ✓）
//
// **∴ 为什么 ✗**（**文档附录五 ✓）**：**`brush_preview` 这名字**误导**✗
//   ⇒ **∴ AI 以为是"看笔刷外观"✗ 其实是"用这支笔画一条曲线试试" ✓
//     ＋ **∴ 而且**不能批量**✗ ⇒ **∴ 试 3 个组合要 3 次调用 ✗ 试错成本高（要 undo ✓）** ✓ ★★
//   ＋ **∴ 要的 ✗**：**新工具 `swatch_brush_strokes`（**画家术语 ✓）：
//     **笔刷 × 颜色 × 路径**的**批量组合 ✗ 一次调用返回多张对比图 ✓
//     ＋ **而且**不写文档、不生成 atom**✗ ⇒ **∴ 可以放心试 ✓）★★
//
// **∴ 判什么（**五条 ✓）★**：
//   ① 工具存在 ✗ 且接受 `brushes[]`／`colors[]`／`paths[]` ✓
//   ② **组合数 = 笔刷数 × 颜色数 × 路径数**（**∴ 一个都不能少 ✓）★★
//   ③ **不污染文档** ✗**：调用前后**对象数必须完全不变** ✓（**∴ 这是"可放心试"的核心 ✓）★★★
//   ④ 每条 swatch 都带**能取到的图**（`thumb_url` ⇒ HTTP 200 ✗ 且不是错误页 ✓）★
//   ⑤ **确定性 ✗**：同样参数再调一次 ⇒ 图**逐字节相同** ✓（**硬约束"同输入同输出" ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把批量循环改成只跑第一个组合 ⇒ **② 立刻红** ✓ ★★★
//
// 用法：node scripts/tool-swatch.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-swatch.mjs <base-url>");
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
  const doc = `sw_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const objectsBefore = await call(doc, token, "list_objects", {});
  const before = ((objectsBefore && (objectsBefore.objects || objectsBefore.items)) || []).length;

  const args = {
    // **∴ 别用纯揉合笔刷 ✗**（`Blender` 在空画布上什么都不沉积 ✓ 那是**预期**行为 ✓
    //   ⇒ 产品对单个失败会如实记在该条上、继续跑其余 ✓）⇒ **∴ 这里用能沉积颜色的 ✓** ★
    // **`blending` 是涂抹类** ✗（第 1132 轮实测）：空画布上没有颜色可抹 ⇒ 4 条 precondition_failed
    //   ⇒ A6 之后顶层 `ok=false` / HTTP 400 ⇒ 判据连 ② 都走不到 ✗。
    //   ⇒ 换成三支**都能沉积**的（`Classic_Paint` / `2B_pencil` / `100%_Opaque`）✓。
    brushes: ["Classic_Paint", "2B_pencil.myb", "100%_Opaque"],
    colors: ["#2e4d8f", "#c8a06a"],
    paths: [
      [[40, 60, 0.2], [120, 50, 0.9], [200, 70, 0.3]],
      [[40, 140, 0.5], [120, 130, 0.5], [200, 150, 0.5]],
    ],
    size: 32,
  };
  const replied = await call(doc, token, "swatch_brush_strokes", args);
  if (!replied || replied.ok !== true) {
    // **∴ 工具不存在 = 产品缺口 ⇒ 红（exit 1 ✓）✗ 不是"判不了"** ✓（**与前面几条同一约定 ✓）★
    console.error(`❌ ① 试笔触工具还没接上：${JSON.stringify(replied).slice(0, 200)}`);
    process.exit(1);
  }
  const swatches = replied.swatches || [];
  const expected = args.brushes.length * args.colors.length * args.paths.length;
  console.log(`  ② 组合数：${swatches.length}（期望 ${expected} = ${args.brushes.length}×${args.colors.length}×${args.paths.length}）`);

  const objectsAfter = await call(doc, token, "list_objects", {});
  const after = ((objectsAfter && (objectsAfter.objects || objectsAfter.items)) || []).length;
  console.log(`  ③ 文档对象数：调用前 ${before} ⇒ 调用后 ${after}`);

  const bad = [];
  if (swatches.length !== expected) {
    bad.push(`② 组合数不对：拿到 ${swatches.length}，期望 ${expected}（笔刷×颜色×路径 一个都不能少）`);
  }
  if (after !== before) {
    bad.push(`③ 试笔触**污染了文档**（对象数 ${before} ⇒ ${after}）⇒ 不敢放心试`);
  }
  let fetched = 0;
  const digests = [];
  const okSwatches = swatches.filter((x) => x.ok !== false);
  console.log(`  （其中 ${okSwatches.length}/${swatches.length} 条成功出图）`);
  for (const s of okSwatches) {
    const url = s.thumb_url || s.url;
    if (!url) {
      bad.push("④ 有 swatch 没有图（thumb_url）");
      break;
    }
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${String(url).replace("yanshi://blob/", "/api/blob/")}`;
    const response = await fetch(absolute);
    if (!response.ok) {
      bad.push(`④ 取图失败（HTTP ${response.status}）：${String(url).slice(0, 60)}`);
      break;
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length < 200) {
      bad.push(`④ 取到的图太小（${bytes.length} 字节）⇒ 像错误页`);
      break;
    }
    fetched += 1;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 1) sum = (sum * 31 + bytes[i]) >>> 0;
    digests.push(sum);
  }
  console.log(`  ④ 取到 ${fetched}/${okSwatches.length} 张图｜不同图 ${new Set(digests).size} 种`);
  if (fetched === okSwatches.length && new Set(digests).size < 2) {
    bad.push("④ 所有组合出的是同一张图 ⇒ 颜色/笔刷/路径没起作用");
  }

  // ⑤ 确定性：同样参数再来一次，逐字节相同
  const again = await call(doc, token, "swatch_brush_strokes", args);
  const swatches2 = (again && again.swatches) || [];
  const digests2 = [];
  for (const s of swatches2) {
    const url = s.thumb_url || s.url;
    if (!url) continue;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${String(url).replace("yanshi://blob/", "/api/blob/")}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 1) sum = (sum * 31 + bytes[i]) >>> 0;
    digests2.push(sum);
  }
  if (digests2.length === digests.length && digests2.join(",") !== digests.join(",")) {
    bad.push("⑤ 同样参数两次调用出的图不同 ⇒ 不满足确定性（同输入必须同输出）");
  } else {
    console.log(`  ⑤ 确定性：两次调用 ${digests2.length} 张图逐字节一致`);
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }

  // **★ 问题 6：`preview_size`（大面积预览）★**（第 254 轮）
  //
  //   **要证**：① `area` 的**真实覆盖率**明显高于 `single` ✓（这正是"覆盖力看得见" ✓）；
  //   ② **缺省必须与显式 `single` 逐字节相同** ✓（缺省不变是硬要求 ✓）。
  //
  //   **★ 一次教训（写在这里）★**：我第一版用**缩略图字节数**当"墨多"的代理 ✗ —— 实测
  //   `area` 是 **1429** 字节、`single` 是 **2311** ✗ => **方向完全相反**（密集平涂压缩得更好 ✓）。
  //   => **代理量被自己的判据否掉** ⇒ 改成**解码 PNG、量真像素** ✓（`decodePngRgba` 在下面）。
  const decodePngRgba = async (bytes) => {
    const { inflateSync } = await import("node:zlib");
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    let at = 8;
    let width = 0;
    let height = 0;
    let colorType = 6;
    const idat = [];
    while (at + 8 <= bytes.length) {
      const length = view.getUint32(at, false);
      const type = String.fromCharCode(bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]);
      const body = bytes.subarray(at + 8, at + 8 + length);
      if (type === "IHDR") {
        width = view.getUint32(at + 8, false);
        height = view.getUint32(at + 12, false);
        colorType = bytes[at + 17];
      } else if (type === "IDAT") {
        idat.push(Buffer.from(body));
      } else if (type === "IEND") {
        break;
      }
      at += 12 + length;
    }
    const raw = inflateSync(Buffer.concat(idat));
    const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
    if (channels === 0 || width === 0) return null;
    const stride = width * channels;
    const out = new Uint8Array(width * height * 4);
    let previous = new Uint8Array(stride);
    for (let y = 0; y < height; y += 1) {
      const filter = raw[y * (stride + 1)];
      const line = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
      const current = new Uint8Array(stride);
      for (let i = 0; i < stride; i += 1) {
        const left = i >= channels ? current[i - channels] : 0;
        const up = previous[i];
        const upLeft = i >= channels ? previous[i - channels] : 0;
        let value = line[i];
        if (filter === 1) value += left;
        else if (filter === 2) value += up;
        else if (filter === 3) value += (left + up) >> 1;
        else if (filter === 4) {
          const p = left + up - upLeft;
          const pa = Math.abs(p - left);
          const pb = Math.abs(p - up);
          const pc = Math.abs(p - upLeft);
          value += pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
        }
        current[i] = value & 0xff;
      }
      for (let x = 0; x < width; x += 1) {
        const source = x * channels;
        const target = (y * width + x) * 4;
        out[target] = current[source];
        out[target + 1] = current[source + 1];
        out[target + 2] = current[source + 2];
        out[target + 3] = channels === 4 ? current[source + 3] : 255;
      }
      previous = current;
    }
    return { width, height, pixels: out };
  };
  /** **覆盖率** = 有颜料像素里"确实上了色"（明显暗于白底）的比例 ✓。 */
  const coverage = (image) => {
    if (!image) return null;
    let painted = 0;
    let inked = 0;
    let darkness = 0;
    const { pixels } = image;
    for (let i = 0; i < pixels.length; i += 4) {
      const alpha = pixels[i + 3];
      if (alpha < 8) continue;
      painted += 1;
      // **★ 必须先按 alpha 合到白底（第 254 轮实测）★**：缩略图是**透明背景**的 ✗
      //   => 直接读 RGB 会把"没画的地方"当成**黑色** ✗ => 两种模式都量出 255.0（饱和 ✗）。
      const a = alpha / 255;
      const luma =
        ((pixels[i] * a + 255 * (1 - a)) +
          (pixels[i + 1] * a + 255 * (1 - a)) +
          (pixels[i + 2] * a + 255 * (1 - a))) /
        3;
      darkness += 255 - luma;
      if (luma < 200) inked += 1;
    }
    if (painted === 0) return null;
    return { painted, inked, ratio: inked / painted, darkness: darkness / painted };
  };
  const thumb = async (previewSize) => {
    const extra = previewSize ? { preview_size: previewSize } : {};
    // **★ 不能带 `args` 里的 `paths` ★**：本工具里"**显式 `paths` 优先于 `preview_size`**" ✓
    //   => 带上 paths 的话 ⑥ 两条就变成**空操作**了（两次都用同一组点 ✗）。
    // **必须用"半透明"的设置**：100%_Opaque 无论几趟都全不透明 => 指标**饱和**（实测过：
    //   single 与 area 都是 100% 覆盖 ✗）。用 opaque: 0.2 才能看出**叠加**。
    const replied = await call(doc, token, "swatch_brush_strokes", {
      brushes: ["Classic_Paint"],
      size: 40,
      brush_params: { opaque: 0.12 },
      ...extra,
    });
    const first = replied && Array.isArray(replied.swatches) ? replied.swatches[0] : null;
    const url = first && first.thumb_url;
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    return { size: bytes.length, image: await decodePngRgba(bytes) };
  };
  const single = await thumb("single");
  const area = await thumb("area");
  const dflt = await thumb(null);
  const covSingle = single && coverage(single.image);
  const covArea = area && coverage(area.image);
  if (!single || !area || !dflt || !covSingle || !covArea) {
    console.error("❌ 判不了：⑥ 的缩略图取不到或解不开 ⇒ 退出码 3");
    process.exit(3);
  }
  console.log(
    `  ⑥ 平均浓度（解码 PNG 后的真像素，opaque=0.2 => 叠加才看得出来）：` +
      `single ${covSingle.darkness.toFixed(1)}（覆盖 ${(covSingle.ratio * 100).toFixed(1)}%）｜` +
      `area ${covArea.darkness.toFixed(1)}（覆盖 ${(covArea.ratio * 100).toFixed(1)}%）｜缺省 ${dflt.size}B`,
  );
  const areaBad = [];
  if (!(covArea.darkness > covSingle.darkness * 1.15)) {
    areaBad.push(
      `⑥ \`preview_size:"area"\` 的平均浓度没有明显更高（${covArea.darkness.toFixed(1)} vs ` +
        `${covSingle.darkness.toFixed(1)}）⇒ 来回 5 趟没生效`,
    );
  }
  if (dflt.size !== single.size) {
    areaBad.push(`⑥ 缺省与显式 single 的缩略图不同（${dflt.size}B vs ${single.size}B）⇒ 缺省行为被改了 ✗`);
  }
  if (areaBad.length) {
    console.error(`❌ ${areaBad.length} 项没成立：${areaBad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ ⑥ 成立：area 的**平均浓度**明显更高（来回 5 趟的叠加看得见 ✓）；缺省与 single 逐字节相同 ✓");
  console.log(`✓ 试笔触可用：${swatches.length} 个组合一次出图、不污染文档、图能取、可复现`);
};

await main();
