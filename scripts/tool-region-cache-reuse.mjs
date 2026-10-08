#!/usr/bin/env node
// **★ 子区域渲染必须能复用 ✓ ★**（外部黑盒报告 2026-10-09 ✓／第 465 轮 ✓）
//
// **它判什么** ✓：**只改**最上面那层**⇒ **同一子区域的第二次渲染必须快一个量级****。
//
// **为什么会有这条** ✗：外部报告实测 —— **全文档重复渲染 1143 → 3.2 ms（~350× ✓）**，
//   而**子区域重复渲染 531 → 426 → 347 ms（~1.5× ✗）**。
//   **∴ 根因（第 465 轮查明 ✓）**：**两条路走**两套缓存**✗** ——
//   * **全文档**命中的是**缩略图／预览**缓存 ✓（**自有"是否最新"判定 ✓**）；
//   * **子区域**走的是 `region_cache` ✓，**而它的新鲜度判据是**裸 `head_seq()`**✗**
//     （`document.rs:943` ✓ ＋ `region_block.rs:116` ✓）⇒ **∴ 只要文档序号变（**哪怕只改最上层 ✓**）
//     ⇒ **∴ 该块立即失效 ⇒ 每次都重渲 ✗**。
//
// **★ 必须带 `raw: true` ✓ ★**（第 469 轮**决定性发现 ✓**）：**`region_cache` 只在
//   `raw: true` 的分支里被查**（`tools.rs` 的 `read_render_region` ✓：`if args["raw"] { … render_region_raw … }` ✓）
//   ⇒ **∴ 不带 `raw` ⇒ 走另一条分支 ⇒ **永远不碰那个缓存 ✗****（**∴ 这解释了外部报告"子区域不命中 ✗"✓**）。
//
// **判据（可红 ✓）**：`t2 <= t1 / 5` ✓（**量级判据 ✓，不是绝对值 ✓ —— 绝对值依赖机器 ✗**）。
// **今天的预期** ✗：**红 ✓**（**实测比值 ~1.5× ✗**）。
// **转绿条件** ✓：**把 `version` 从**裸 `head_seq`**✗ 换成**影响该区域的内容指纹****
//   （**＝ below 缓存那套 ✓：层参数 ＋ 对象 `current_version`／`data` ＋ 蒙版 ＋ 色彩空间 ✓**）。
// **变异** ✗：**改回裸 `head_seq` ⇒ 比值回到 ~1.5× ⇒ 必红 ✓**。
//
// 用法：node scripts/tool-region-cache-reuse.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-region-cache-reuse.mjs <base-url>"); process.exit(2); }

const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};

const main = async () => {
  const doc = "rr_" + Date.now().toString(36);
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 1024, height: 1024 }),
  })).json()).token;

  const ids = ["R0", "R1", "R2", "Rtop"];
  for (const id of ids) {
    await call(doc, token, "create_layer", { layer_id: id, name: id });
  }
  const region = { x: 128, y: 128, w: 512, h: 512 };
  const stroke = (layer) => call(doc, token, "brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", size: 24,
    points: [[200, 200, 1.0], [400, 400, 1.0]],
    color: { r: 30, g: 120, b: 220, a: 255 }, preview: false,
  });

  // 铺一点内容 ✓（下方各层各一笔 ✓）
  for (const id of ids.slice(0, 3)) await stroke(id);
  await stroke("Rtop");

  const t0 = Date.now();
  await call(doc, token, "render_region", { region, raw: true });
  const t1 = Date.now() - t0;

  // **只改最上面那层** ✓ ⇒ 下方不变 ⇒ 子区域缓存**应当**仍然有效 ✓
  await stroke("Rtop");

  const t2s = Date.now();
  await call(doc, token, "render_region", { region, raw: true });
  const t2 = Date.now() - t2s;

  const h = await (await fetch(`${base}/health`)).json();
  const ratio = t1 > 0 ? t2 / t1 : null;
  console.log(`  同一子区域（128,128,512×512 ✓）`);
  console.log(`    第一次 ${t1} ms｜**只改最上层后**第二次 ${t2} ms｜比值 ${ratio === null ? "n/a" : ratio.toFixed(2)}×`);
  console.log(`    /health.below_reuse=${JSON.stringify(h.below_reuse)}`);

  if (ratio === null || t1 < 20) {
    console.error("❌ 第一次太快 ⇒ 量不出复用 ⇒ 判据无效 ⇒ 不许当通过 ✗（换更大的区域／更多层 ✓）");
    process.exit(1);
  }
  if (ratio > 0.2) {
    console.error(`❌ 只改最上层后，同一子区域第二次仍要 ${t2} ms（比值 ${ratio.toFixed(2)}× > 0.2）` +
      ` ⇒ 子区域**没有**复用 ✗（**根因：version 用的是裸 head_seq ✗**）` +
      `（变异：改回裸 head_seq ⇒ 必红 ✓）`);
    process.exit(1);
  }
  console.log(`  ✓ 子区域复用生效（比值 ${ratio.toFixed(2)}× ≤ 0.2）`);
};

main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
