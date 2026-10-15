#!/usr/bin/env node
// **★ `render_region {show_coords}`：导出坐标水印（Muse 建议 4，P2）★**
//
// **它解决的那一点** ✓：`render_region` **已经**回报 `requested_region` / `rendered_region` /
// `clipped` ✓（所以"取偏了"**能靠数据发现** ✓）—— 但**从图上看不出来** ✗：
// 水印是给**人眼/AI 看图**时的自证 ✓（像"把眉毛切在边缘"这种，图上多一行 `x=0..200 y=0..80` 一目了然 ✓）。
//
// **判据要证的三件事**：
//   ① **默认不叠**（`show_coords` 缺省 false ✓）⇒ 与"完全不传这个参数"**逐字节相同** ✓
//      —— 这是硬约束"默认出图干净"的判据化 ✓；
//   ② **打开后画面确实变了** ✓（不是静默忽略 ✗ —— 本仓库把"参数被静默忽略"列为最糟的 UX 之一 ✓）；
//   ③ **水印只动角落** ✓：除角落带外，其余像素**逐字节不变** ✓
//      —— 免得"加了水印却把画改了" ✗（这是必须钉住的不变式 ✓）。
//
// 用法：node scripts/tool-show-coords.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-show-coords.mjs <base-url>");
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

const pixels = async (url) => {
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  return new Uint8Array(await (await fetch(absolute)).arrayBuffer());
};

const main = async () => {
  const bad = [];
  const doc = `coords_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 400, height: 300 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不出文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });
  // **`fill_region` 的参数是 `{layer_id, shape, feather, color, opacity, texture}`** ✓
  //   （我第一版按 `region`/`object_id` 写 ✗ ⇒ 报「不接受参数 object_id」⇒ 判据自己退 3 ✓
  //     —— 这正是"判据说得出自己坏了"那条纪律 ✓，而不是误判成产品问题 ✓）。
  const filled = await call(doc, token, "fill_region", {
    layer_id: "L1",
    // 扁平写法 ✓（`{kind,bbox}` 那种我试过 ✗ ⇒ 报「宽高必须大于 0（w=0、h=0）」✓）
    shape: { kind: "rect", x: 0, y: 0, w: 400, h: 300 },
    color: { r: 240, g: 235, b: 225, a: 255 },
  });
  if (filled.ok !== true) {
    console.error(`❌ 判不了：fill_region 前置失败 ⇒ ${JSON.stringify(filled).slice(0, 160)} ⇒ 退出码 3`);
    process.exit(3);
  }

  const region = { x: 40, y: 30, w: 200, h: 150 };
  const plain = await call(doc, token, "render_region", { region, raw: true });
  const shown = await call(doc, token, "render_region", { region, raw: true, show_coords: true });
  const plainUrl = plain.raw_url || plain.rawUrl;
  const shownUrl = shown.raw_url || shown.rawUrl;
  if (!plainUrl || !shownUrl) {
    // **★ 分清"产品缺口"与"判据自己坏了" ★**：本仓库**拒绝未知参数** ✓ ⇒ 今天传
    //   `show_coords` 会让**整个调用失败** ✗（实测原文正是「不接受参数 show_coords」✓）。
    //   那是**产品缺口**（Muse 建议 4 未实现 ✓）=> 退 **1** ✓；只有"连不带新参数的调用都失败"
    //   才算判据的前置不成立 ⇒ 退 **3** ✓（这就是"判据说得出自己坏了"那条纪律 ✓）。
    const detail = JSON.stringify(shown).slice(0, 200);
    if (!plainUrl) {
      console.error(`❌ 判不了：连基准调用（不带 show_coords）都拿不到像素 ⇒ ${detail} ⇒ 退出码 3`);
      process.exit(3);
    }
    console.error(
      `❌ ② \`show_coords\` 让调用失败了 ⇒ 参数尚未实现（产品缺口，Muse 建议 4）\n` +
        `   实测原文：${detail}`,
    );
    process.exit(1);
  }
  const a = await pixels(plainUrl);
  const b = await pixels(shownUrl);
  console.log(`  区域 ${JSON.stringify(region)}｜像素 ${a.length} / ${b.length} 字节`);
  if (a.length !== b.length) {
    bad.push(`① 两张图尺寸不一致（${a.length} vs ${b.length}）⇒ 水印不该改尺寸 ✗`);
  }

  // ① 默认不叠：与不传参数逐字节相同
  const again = await call(doc, token, "render_region", { region, raw: true, show_coords: false });
  const c = await pixels(again.raw_url || again.rawUrl);
  let sameAsPlain = a.length === c.length;
  for (let i = 0; sameAsPlain && i < a.length; i += 1) sameAsPlain = a[i] === c[i];
  console.log(`  ① show_coords: false ⇒ 与不传参数逐字节相同=${sameAsPlain}`);
  if (!sameAsPlain) bad.push("① `show_coords: false` 必须与不传参数**逐字节相同** ✗（默认出图要干净 ✓）");

  // ② 打开后确实变了
  let differences = 0;
  for (let i = 0; i < Math.min(a.length, b.length); i += 1) if (a[i] !== b[i]) differences += 1;
  console.log(`  ② show_coords: true ⇒ 与默认相比 ${differences} 个字节不同`);
  if (differences === 0) {
    bad.push(
      "② `show_coords: true` **什么都没做** ⇒ 参数被静默忽略 ✗" +
        "（本仓库把「参数被静默忽略」列为最糟的 UX 之一 ✓）",
    );
  }

  // ③ 水印只动角落：把画布切成"角落带"与"其余"，其余必须逐字节不变
  const width = plain.width || shown.width || 200;
  const height = plain.height || shown.height || 150;
  const band = Math.max(24, Math.round(height * 0.25));
  let outsideChanged = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const at = (y * width + x) * 4;
      if (at + 3 >= Math.min(a.length, b.length)) continue;
      const inCorner = y < band && x < width; // 顶部一条带（水印在角落 ✓）
      if (!inCorner && (a[at] !== b[at] || a[at + 1] !== b[at + 1] || a[at + 2] !== b[at + 2])) {
        outsideChanged += 1;
      }
    }
  }
  console.log(`  ③ 角落带之外被改动的像素=${outsideChanged}（应 0 ✓）`);
  if (outsideChanged > 0) {
    bad.push(
      `③ 水印动到了角落带之外的 ${outsideChanged} 个像素 ✗ ⇒ 加个水印不该把画改了 ✓`,
    );
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    "✓ 坐标水印成立：缺省逐字节不变 ✓、打开后确实叠上 ✓、且只动角落 ✓",
  );
};

await main();
