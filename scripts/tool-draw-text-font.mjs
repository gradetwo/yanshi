#!/usr/bin/env node
// **★ 服务端真字体：`list_fonts` ＋ `draw_text{font:<族名>}`（字体方案 Phase 3 ✓）★**（**第 254 轮 ✓）
//
// **∴ 现状（**读代码 ✓）✗**：**内核只把 `font` 字段**存进对象数据**✗
//   ＋ **∴ 渲染时**无论什么值都用内嵌的 16×16 位图图集**✗（**`font_atlas::Atlas::builtin()` ✓）
//     ⇒ **∴ 于是**：**服务端（**MCP／无头 ✓）**选字体**完全没有效果** ✓ ★★
//
// **∴ Phase 3 的设计（**不碰内核 ✗ 不让 wasm 背依赖 ✓）★**：
//   **∴ 服务端**自己栅格化**（`fontdb` 枚举 ＋ `ab_glyph` 光栅化 ✓）⇒ 产出位图 ⇒ `import_image`** ✓
//     ⇒ **∴ 与 Phase 1 的 Web 路径**完全同构** ✓（**∴ 内核与 wasm 一行不改 ✓）★★
//
// **∴ 判什么（**四条 ✓）★**：
//   ① `list_fonts` 能列出**至少两支**可用字体（**∴ 判据**自己从它挑**✗ 不写死族名 ✓）★
//   ② 同一个字符串 ✗ 用**两支不同**的真实字体画 ⇒ **墨迹指纹必须不同** ✓ ★★★
//   ③ 两次都**真的画上了**（**∴ 否则"都画不上"也会相同 ⇒ 假绿 ✓）★
//   ④ `font:"builtin"` **仍然可用** ✗ 且与真字体**不同**（**∴ 老路径零回归 ✓）★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 `font` 参数忽略掉（**永远走内置图集 ✓）
//   ⇒ **∴ ② 立刻红** ✓
//
// **∴ 本判据**不做**什么 ✗**：**不判**字形好不好看**✗ **不判**具体字体文件 ✓**（**只判"参数真的起作用" ✓）★
//
// 用法：node scripts/tool-draw-text-font.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-draw-text-font.mjs <base-url>");
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
  const doc = `dtf_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 512, height: 320 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const layer = await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  if (!layer || layer.ok !== true) {
    console.error(`❌ 判不了：建不了图层 ⇒ 退出码 3`);
    process.exit(3);
  }

  // **① 枚举系统字体** ✓
  const fonts = await call(doc, token, "list_fonts", {});
  // **★ 约定：**缺功能 ⇒ **红（exit 1）**✗ 不是"判不了"（退 3）** ✓ ★**（**第 254 轮自查修 ✓）
  //   ＋ **∴ 为什么 ✗**：**退 3 的意思是"**判据自己跑不了**✗ 而不是"**产品还没做** ✓
  //     ＋ **∴ 若**这里退 3 ✗ ⇒ **∴ 缺 `list_fonts` 这件产品缺口**会**蒙混过关** ✓ ★★
  if (!fonts || fonts.ok !== true) {
    console.error(`❌ ① 服务端没有字体枚举入口（list_fonts 不成立：${JSON.stringify(fonts).slice(0, 180)}）`);
    process.exit(1);
  }
  const list = (fonts.fonts || []).map((f) => (typeof f === "string" ? f : f.family)).filter(Boolean);
  const unique = [...new Set(list)];
  console.log(`  ① list_fonts：${unique.length} 支（示例：${unique.slice(0, 4).join("｜")}）`);
  if (unique.length < 2) {
    console.error(`❌ 判不了：这台机器只枚举到 ${unique.length} 支字体 ⇒ 凑不出两支来比 ⇒ 退出码 3`);
    process.exit(3);
  }

  // **∴ 取墨迹指纹 ✗**（**∴ 按对象 bbox 取样 ✗ 只数墨 ✓ 不看整幅 ✓）★
  const fingerprintOf = async (objectId) => {
    const objects = await call(doc, token, "list_objects", {});
    const target = ((objects && (objects.objects || objects.items)) || []).find(
      (o) => String(o.object_id || o.id) === objectId,
    );
    const bb = target && (target.bbox || target.bounds);
    const region = Array.isArray(bb) && bb.length === 4
      ? { x: Math.max(0, Math.floor(bb[0]) - 4), y: Math.max(0, Math.floor(bb[1]) - 4), w: Math.max(1, Math.ceil(bb[2]) + 8), h: Math.max(1, Math.ceil(bb[3]) + 8) }
      : { x: 0, y: 0, w: 512, h: 320 };
    const raw = await call(doc, token, "render_region", { region, raw: true });
    const url = raw && (raw.raw_url || raw.rawUrl);
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const bytes = new Uint8Array(await (await fetch(absolute)).arrayBuffer());
    let ink = 0;
    let sum = 0;
    for (let i = 0; i < bytes.length; i += 4) {
      const white = bytes[i] >= 250 && bytes[i + 1] >= 250 && bytes[i + 2] >= 250;
      if (!white) ink += 1;
      sum = (sum * 31 + bytes[i] + bytes[i + 1] * 3 + bytes[i + 2] * 7) >>> 0;
    }
    return { ink, fingerprint: sum, region };
  };

  const draw = async (objectId, font) => {
    const replied = await call(doc, token, "draw_text", {
      layer_id: "L1",
      object_id: objectId,
      data: {
        text: "FontAB",
        font,
        size: 48,
        color: "#101010",
        position: [40, 40 + (objectId === "t1" ? 0 : 90)],
        align: "left",
      },
    });
    const measured = replied && replied.ok === true ? await fingerprintOf(objectId) : null;
    console.log(
      `  ${font === "builtin" ? "内置位图" : "字体 " + font}：ok=${replied && replied.ok}｜墨 ${measured ? measured.ink : "?"}｜指纹 ${measured ? measured.fingerprint : "?"}｜区域 ${measured ? JSON.stringify(measured.region) : "?"}`,
    );
    return { replied, measured };
  };

  // **★★★ 不能只试"排序后的头两支" ✗ ★★★**（**第 254 轮实测 ✓）
  //   ＋ **∴ 踩到的坑 ✗**：**我原来取 `unique[0]`／`unique[1]`**✗ **而**这台机器上前两支是
  //     `AR PL KaitiM Big5` 与 `AR PL KaitiM GB`**✗ —— **同一设计的不同编码** ✓
  //     ⇒ **∴ 它们画 "FontAB" **真的逐字节相同**✓ ⇒ **∴ 判据**误报"font 没生效"** ✓ ★★
  //   ＋ **∴ 所以 ✗**：**最多试 6 支 ✗ 只要出现**两种以上**指纹就算"参数真的起作用"** ✓
  //     ＋ **∴ 而**变异（忽略 font ✓）⇒ **六支全同 ⇒ 只有 1 种 ⇒ 仍然红** ✓ ★★★
  const samples = unique.slice(0, 6);
  const drawn = [];
  for (let i = 0; i < samples.length; i += 1) {
    drawn.push(await draw("t" + i, samples[i]));
  }
  // ④ builtin（老路径零回归）
  const c = await draw("tbuiltin", "builtin");
  const distinct = new Set(drawn.filter((d) => d.measured).map((d) => d.measured.fingerprint));
  console.log(`  ② 试了 ${samples.length} 支字体 ⇒ 出现 ${distinct.size} 种指纹`);

  const bad = [];
  const failures = drawn.filter((d) => !d.replied || d.replied.ok !== true || !d.measured);
  if (failures.length) bad.push(`② 有 ${failures.length} 支字体画不出来`);
  if (!c.replied || c.replied.ok !== true || !c.measured) bad.push("④ font=builtin 画不出来（老路径回归）");
  const inked = drawn.filter((d) => d.measured && d.measured.ink >= 30);
  if (!inked.length) {
    bad.push("③ 所有真字体画面上都没有墨 ⇒ 这条比较没有说服力");
  } else if (distinct.size < 2) {
    bad.push(`② 试了 ${samples.length} 支不同字体，指纹只有 ${distinct.size} 种 ⇒ font 参数没有真的生效`);
  }
  if (c.measured && inked.length && c.measured.fingerprint === inked[0].measured.fingerprint) {
    bad.push("④ builtin 与真字体画出来相同 ⇒ 真字体那条路其实没走");
  }
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  // **∴ 成功文案要**如实**✗ 不能说"头两支不同"**✓（**∴ 它们可能真是同一设计 ✓）★
  console.log(
    `✓ 服务端真字体生效：试了 ${samples.length} 支出现 ${distinct.size} 种不同墨迹，builtin 仍可用且与它们不同`,
  );
};

await main();
