#!/usr/bin/env node
// **★ 防撒谎判据（设计 §5 的 C4 ✓）★**：**改下方任何层 ⇒ below 缓存**必须**失效 ⇒ 输出必须变** ✓
//
// **它判什么** ✓（**三条断言，逐条可红 ✓**）：
//   **① 改最**下层**⇒ 输出**必须变**** ✓（**∴ 若缓存把旧的下方合成冒充 ⇒ 输出不变 ⇒ 必红 ✓**）；
//   **② 改最**上层**⇒ 输出**必须变**** ✓（**∴ 复用了 below ✓ 但当前层要重画 ✓**）；
//   **③ 无改动再渲染 ⇒ 输出**必须完全一致**** ✓（**∴ 确定性 ✓，且不能"每次都变"✗**）。
//
// **为什么单独立一条** ✗：`tool-below-reuse.mjs` 判的是 C3（**只改当前层 ⇒ 要复用 ✓**），
//   而本条判 **C4（**改下层 ⇒ 必须失效 ✓**）** —— **∴ 两条合起来才封住"撒谎"✗**。
//
// **今天的预期** ✓：**绿 ✓**（**∵ 指纹覆盖**层参数 ＋ 对象 `current_version`／`data` ＋ 蒙版 ＋ 色彩空间 ✓**）。
// **变异** ✗：**让指纹**忽略下方层**（**如只算最上层 ✓**）⇒ 断言 ① 必红 ✓**。
//
// 用法：node scripts/tool-below-invalidation.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-below-invalidation.mjs <base-url>"); process.exit(2); }

const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};

const main = async () => {
  const doc = "bi_" + Date.now().toString(36);
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
  })).json()).token;

  const ids = ["B0", "B1", "Btop"];
  for (const id of ids) await call(doc, token, "create_layer", { layer_id: id, name: id });
  const region = { x: 0, y: 0, w: 512, h: 512 };
  const stroke = (layer, dx) => call(doc, token, "brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", size: 40,
    points: [[100 + dx, 100, 1.0], [300 + dx, 300, 1.0]],
    color: { r: 20, g: 200, b: 90, a: 255 }, preview: false,
  });
  // **取"渲染指纹"** ✓：`raw: true` 回的 `raw_url` 里带 blob 哈希 ✓（不必取像素 ✓）。
  const finger = async () => {
    const r = await call(doc, token, "render_region", { region, raw: true });
    return String(r.raw_url || r.preview_url || JSON.stringify(r).slice(0, 80));
  };

  for (const id of ids) await stroke(id, 0);
  const h1 = await finger();

  // **不做任何改动** ✓ ⇒ 断言 ③
  const h2 = await finger();

  // **改最下层** ✓ ⇒ 断言 ①
  await stroke("B0", 12);
  const h3 = await finger();

  // **改最上层** ✓ ⇒ 断言 ②
  await stroke("Btop", 24);
  const h4 = await finger();

  console.log(`  h1（初值）= ${h1}`);
  console.log(`  h2（无改动）= ${h2}`);
  console.log(`  h3（改最下层后）= ${h3}`);
  console.log(`  h4（改最上层后）= ${h4}`);

  const bad = [];
  if (h2 !== h1) bad.push("③ 无改动再渲染 ⇒ 输出应当**完全一致** ✗（实测变了 ⇒ 不确定性 ✗）");
  if (h3 === h2) bad.push("① 改**最下层** ⇒ 输出**必须变** ✗（实测没变 ⇒ **below 缓存把旧合成冒充了 ⇒ 撒谎 ✓**）");
  if (h4 === h3) bad.push("② 改**最上层** ⇒ 输出**必须变** ✗");
  if (bad.length) {
    console.error("❌ " + bad.length + " 条不成立：");
    for (const b of bad) console.error("   - " + b);
    console.error("（变异：让指纹忽略下方层 ⇒ 断言 ① 必红 ✓）");
    process.exit(1);
  }
  console.log("  ✓ 改下层 ⇒ 失效 ✓｜改上层 ⇒ 变化 ✓｜无改动 ⇒ 一致 ✓（**∴ 缓存不撒谎 ✓**）");
};

main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
