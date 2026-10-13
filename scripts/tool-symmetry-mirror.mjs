#!/usr/bin/env node
// **★ 对称模式：垂直镜像 ⇒ 左右像素镜像差为 0 ✗ ★**（**第 251 轮 ✓）
//
// **∴ 判什么（**三件 ✗ 成对 ⇒ 有区分力 ✓）★**：
//   **∴ ① 有效样本 ✗**：**这一笔**真的画上了像素**（**∴ 否则一张全空的图**天然左右相等**
//     ⇒ **∴ 判据会**假绿** ✓）★
//   **∴ ② 镜像 ✗**：**`symmetry="vertical"` ⇒ **左右两半逐像素逐通道差为 0** ✓
//   **∴ ③ 对照 ✗**：**同样的笔**不开对称 ⇒ **左右不该相等** ✓（**∴ 排除"恒真" ✓）★
//
// **∴ 为什么走**服务端 `brush_stroke`**✗ 而不是 `draw_stroke`** ✓：
//   ＋ **∴ 仓库自己写明 ✗**：**"提交一条笔触走的是**本文件这一份**（**`hokusai` ✓）
//     ＋ **∴ 而** `yanshi-render::brush` 是**渲染那条路**用的** ✓
//     ⇒ **∴ 两条路**都要接 ✗ ⇒ **∴ 这条判据专门盯**UI 走的那条** ✓ ★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**服务端不做镜像**（**忽略 `symmetry` ✓）⇒ **∴ ② 必红** ✓
//
// 用法：node scripts/tool-symmetry-mirror.mjs <base-url>
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-symmetry-mirror.mjs <base-url>");
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
  const doc = "sym_" + Date.now().toString(36);
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error(`❌ 判不了：建不了文档（${JSON.stringify(created).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const token = created.token;
  await call(doc, token, "create_layer", { layer_id: "L1", name: "L1" });

  // **∴ 用**原始 RGBA**读回来 ✗ 逐像素自己比** ✓（**∴ 不靠任何文字结论 ✓）★
  // **∴ 取原始像素 ✗**：**`render_region` 给的是 **`raw_url`**（**实测回执键 ✓）
  //   ＋ **∴ 我第一版**去猜 `blob_hash`**✗ ⇒ **∴ 判据**自己退 3** ✓（**∴ 不是产品错 ✓）★
  const fetchRaw = async (value) => {
    const url = value && (value.raw_url || value.rawUrl);
    if (!url) return null;
    const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
    const response = await fetch(absolute);
    if (!response.ok) return null;
    return new Uint8Array(await response.arrayBuffer());
  };
  const renderRaw = async () => {
    const value = await call(doc, token, "render_region", {
      region: { x: 0, y: 0, w: 256, h: 256 },
      raw: true,
    });
    return fetchRaw(value);
  };
  const painted = (rgba) => {
    let count = 0;
    for (let i = 3; i < rgba.length; i += 4) if (rgba[i] !== 0) count += 1;
    return count;
  };
  /** **左右两半的最大通道差 ✗**（**∴ 全 0 ⇒ 严格镜像 ✓）★ */
  const mirrorDiff = (rgba, width, height) => {
    let worst = 0;
    for (let y = 0; y < height; y++) {
      for (let x = 0; x < width / 2; x++) {
        const mx = width - 1 - x;
        const a = (y * width + x) * 4;
        const b = (y * width + mx) * 4;
        for (let c = 0; c < 4; c++) {
          const d = Math.abs(rgba[a + c] - rgba[b + c]);
          if (d > worst) worst = d;
        }
      }
    }
    return worst;
  };

  // **∴ ① 开对称画一笔 ✗**：**左半边 ✗ 笔够大 ⇒ 镜像那半也要有墨** ✓ ★
  const args = {
    layer_id: "L1",
    object_id: "sym_obj",
    brush: "100%_Opaque",
    size: 24,
    points: [
      [80, 128, 1.0],
      [80, 160, 1.0],
    ],
    color: { r: 20, g: 20, b: 200, a: 255 },
    preview: false,
  };
  const drawn = await call(doc, token, "brush_stroke", Object.assign({ symmetry: "vertical" }, args));
  if (!drawn || drawn.ok !== true) {
    console.error(`❌ 判不了：对称那一笔没落下（${JSON.stringify(drawn).slice(0, 200)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const symmetric = await renderRaw();
  if (!symmetric) {
    console.error("❌ 判不了：取不回渲染的原始像素 ⇒ 退出码 3");
    process.exit(3);
  }
  const inked = painted(symmetric);
  const bad = [];
  if (inked < 200) bad.push(`① 有效样本不成立：只画上 ${inked} 个像素（太少 ⇒ 后面的比较没意义）`);
  const worst = mirrorDiff(symmetric, 256, 256);
  if (worst !== 0) bad.push(`② 垂直对称下左右必须逐位相等 ✗（实测最大通道差 ${worst}）`);

  // **∴ ③ 对照 ✗**：**另起一个文档 ✗ 同样一笔但**不开对称** ✓ ★
  const doc2 = doc + "_ctl";
  const created2 = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc2, width: 256, height: 256 }),
    })
  ).json();
  if (!created2 || !created2.token) {
    console.error("❌ 判不了：建不了对照文档 ⇒ 退出码 3");
    process.exit(3);
  }
  await call(doc2, created2.token, "create_layer", { layer_id: "L1", name: "L1" });
  await call(doc2, created2.token, "brush_stroke", Object.assign({}, args, { object_id: "plain_obj" }));
  const value2 = await call(doc2, created2.token, "render_region", {
    region: { x: 0, y: 0, w: 256, h: 256 },
    raw: true,
  });
  const controlPixels = await fetchRaw(value2);
  const controlWorst = controlPixels ? mirrorDiff(controlPixels, 256, 256) : null;
  if (controlWorst === null) {
    console.error("❌ 判不了：取不回对照的像素 ⇒ 退出码 3");
    process.exit(3);
  }
  if (controlWorst === 0) bad.push("③ 对照不成立：不开对称时左右**也**相等 ⇒ 这条判据没有区分力");

  console.log(`  ① 对称那一笔落在 ${inked} 个像素上`);
  console.log(`  ② 左右最大通道差 = ${worst}（要求 0）`);
  console.log(`  ③ 对照（不开对称）最大通道差 = ${controlWorst}（要求 > 0）`);
  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log("✓ 垂直对称：左右逐位镜像相等；不开对称则不等（对照成立）");
};

await main();
