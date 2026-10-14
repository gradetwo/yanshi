#!/usr/bin/env node
// **★ carried_color：脏笔拾取（湿画法）到底灵不灵 —— 用像素说话 ★**（第 254 轮 ✓）
//
// **∴ 为什么要它 ✗**：**需求文档（P0 #1 ✓）说"Blender 揉合笔刷无效，只能做 Source-Over"** ✓
//   ＋ **∴ 但读代码发现链路其实是**接上**的 ✗**：
//     `write_brush_stroke`（MCP 的 brush_stroke）传 **`feed_base = true`** ✓
//       ⇒ **`reads_canvas = feed_base && brush_reads_the_canvas(brush)`** ✓
//         （**`brush_reads_the_canvas`：`Smudge / Colorize / Posterize / LockAlpha > 0`** ✓）
//           ⇒ **∴ 所以"到底还灵不灵"**不该靠文档也不该靠读代码 ✗ 得**量** ✓ ★★★
//   ＋ **∴ 文档基线是 `7985e3a9` ✗** 而这段读画布很可能**之后才加**（注释里写着第 245／462 轮 ✓）
//     ⇒ **∴ 这正是"先读代码核对现状是否仍成立"的实例** ✓ ★★
//
// **∴ 判什么（**同一块地方，三笔画三次 ✗ 比像素** ✓）★**：
//   ① 先用**不透明红**画一条**粗**线（`100%_Opaque` ✓）⇒ 记为"底下那层红" ✓
//   ② 同一条线上，用**蓝**画 `blend+paint`（**带色的揉合笔刷** ✓）
//      ⇒ **∴ 若拾取生效 ✗**：**蓝里必然**带出红**（**`r` 明显 > 0 ✓ 红蓝同高** ✓）
//      ⇒ **∴ 若没生效 ✗**：**就是**纯蓝盖上去**（**`r` ≈ 0** ✓）
//   ③ 再用 `Blender`（**纯揉合、不带色** ✓）在同一块上扫一遍
//      ⇒ **∴ 若生效 ✗**：**它会把颜色**拖走／混匀**⇒ 与只画 ② 的结果**不同** ✓
//      ⇒ **∴ 若不生效 ✗**：**它在**空 surface** 上什么也沉积不出来 ⇒ 画面**逐字节不变** ✓
//
// **∴ 能红的根据（**变异 ✓）✗**：**把 `feed_base` 改成 false**（**或把 `reads_canvas` 短路 ✓）
//   ⇒ **∴ ② 的 `r` 掉到 ~0 ✗ ③ 的画面与 ② 逐字节相同** ⇒ 立刻红** ✓ ★★★
//
// **∴ 本判据**不判**什么 ✗**：**不判**好不好看、不判具体混色比例 ✓（**只判"有没有拾取" ✓）★
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

const main = async () => {
  const doc = `cc_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const layer = await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  if (!layer || layer.ok !== true) {
    console.error("❌ 判不了：建不了图层 ⇒ 退出码 3");
    process.exit(3);
  }

  const line = [[80, 260, 1.0], [200, 258, 1.0], [320, 262, 1.0], [440, 260, 1.0]];
  const paint = async (brush, color, points) => {
    const replied = await call(doc, token, "brush_stroke", {
      layer_id: "L1",
      object_id: `o_${brush}_${Math.round(color.r)}`,
      brush,
      size: 48,
      color,
      opacity: 1.0,
      points,
    });
    return replied;
  };
  const sample = async () => {
    const raw = await call(doc, token, "render_region", {
      region: { x: 200, y: 230, w: 120, h: 60 },
      raw: true,
    });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let r = 0;
    let g = 0;
    let b = 0;
    let ink = 0;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      if (bytes[i + 3] > 8) {
        r += bytes[i];
        g += bytes[i + 1];
        b += bytes[i + 2];
        ink += 1;
      }
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { r, g, b, ink, fingerprint: sum };
  };

  // ① 底下那层红
  const red = await paint("100%_Opaque", { r: 220, g: 30, b: 30, a: 255 }, line);
  if (!red || red.ok !== true) {
    console.error(`❌ 判不了：底稿红笔画不上（${JSON.stringify(red).slice(0, 140)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const base1 = await sample();
  if (!base1) {
    console.error("❌ 判不了：取不回像素 ⇒ 退出码 3");
    process.exit(3);
  }
  console.log(`  ① 底稿（红）：墨 ${base1.ink}｜均值 rgb(${(base1.r / Math.max(1, base1.ink)).toFixed(0)},${(base1.g / Math.max(1, base1.ink)).toFixed(0)},${(base1.b / Math.max(1, base1.ink)).toFixed(0)})`);

  // ② 带色揉合笔刷：蓝
  const mixed = await paint("blend+paint", { r: 20, g: 40, b: 220, a: 255 }, line);
  if (!mixed || mixed.ok !== true) {
    console.error(`❌ 判不了：blend+paint 画不上（${JSON.stringify(mixed).slice(0, 140)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const after2 = await sample();
  const mean2 = (k) => after2[k] / Math.max(1, after2.ink);
  console.log(`  ② blend+paint（蓝）之后：墨 ${after2.ink}｜均值 rgb(${mean2("r").toFixed(0)},${mean2("g").toFixed(0)},${mean2("b").toFixed(0)})`);

  // ③ 纯揉合：Blender
  const blended = await paint("Blender", { r: 20, g: 40, b: 220, a: 255 }, line);
  const after3 = blended && blended.ok === true ? await sample() : null;
  if (after3) {
    console.log(`  ③ Blender 再扫一遍之后：墨 ${after3.ink}｜指纹 ${after3.fingerprint}（② 时 ${after2.fingerprint}）`);
  }

  const bad = [];
  // **∴ ② 的判据 ✗**：**带色揉合笔刷画在**红底**上 ⇒ 蓝里必须带红** ✓
  //   ＋ **∴ 阈值怎么定 ✗**：**纯蓝覆盖 ⇒ r ≈ 20**✗ **拾取到红 ⇒ r 会明显上去** ✓
  //     ⇒ **∴ 取 r ≥ 60 作为"带出来了"的下界** ✓（**保守 ✓ 实测绿时远高于它 ✓）★
  if (mean2("r") < 60) {
    bad.push(
      `② 蓝的揉合笔刷画在红底上，结果里几乎没有红（均值 r=${mean2("r").toFixed(0)}）` +
        `⇒ **脏笔拾取没生效**（读画布那条路没起作用）`,
    );
  }
  if (!after3) {
    bad.push(`③ Blender 那次没成功（${JSON.stringify(blended).slice(0, 140)}）`);
  } else if (after3.fingerprint === after2.fingerprint) {
    bad.push("③ 纯揉合笔刷（Blender）扫过之后画面**逐字节没变** ⇒ 它什么也没拾取/拖动（在空 surface 上落笔的老症状）");
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ 脏笔拾取生效：带色揉合笔刷把底色带了出来（r=${mean2("r").toFixed(0)}）` +
      `，纯揉合笔刷也真的改动了画面（指纹 ${after2.fingerprint} ⇒ ${after3.fingerprint}）`,
  );
};

await main();
