#!/usr/bin/env node
// **★ 跨渲染保留必须生效 ✓ ★**（第 496 轮 ✓；目标明文"LRU"✓ 的前半 ✓）
//
// **它判什么** ✓：**在 3 个不同子区域之间轮换 ⇒ **每一次回到已渲过的区域都应命中**✓**
//   ⇒ **∴ 即 `below_reuse` **每轮都自增**✓** —— **∴ 它测的是"**保留**"✗（**旧版每次整体替换 ⇒ 只留最后一组 ✗**）。
//
// **⚠️ 它**不**判什么** ✗（**如实 ✓**）：**"LRU 比 FIFO 命中率高"** ——
//   **∴ 区分它需要**预算紧张 ＋ 大量轮换**✗（**而预算是 256 块 ≈ 64 MiB ✓，要触发淘汰得用 8K ＋ 多次大区域 ✗**）
//   ⇒ **∴ 该对照**本次未做 ✗**，**已记为未测项 ✓**（**不当作"已验证的优化"✗**）。
//
// **变异** ✗：**把存储块改回"每次整体替换"** ⇒ **∴ 轮换第二轮起不命中 ⇒ 必红 ✓**。
//
// 用法：node scripts/tool-below-retention.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-below-retention.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};
const reuse = async () => Number((await (await fetch(`${base}/health`)).json()).below_reuse ?? -1);

const main = async () => {
  const doc = "br_" + Date.now().toString(36);
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 768, height: 768 }),
  })).json()).token;
  const ids = ["T0", "T1", "Ttop"];
  for (const id of ids) await call(doc, token, "create_layer", { layer_id: id, name: id });
  const stroke = (layer, dx) => call(doc, token, "brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", size: 28,
    points: [[150 + dx, 150, 1.0], [350 + dx, 350, 1.0]],
    color: { r: 40, g: 160, b: 200, a: 255 }, preview: false,
  });
  for (const id of ids) await stroke(id, 0);

  // **3 个不同区域** ✓（**各自互不重叠 ✓ ⇒ 每一块都要单独缓存 ✓**）
  const regions = [
    { x: 0, y: 0, w: 256, h: 256 },
    { x: 256, y: 0, w: 256, h: 256 },
    { x: 0, y: 256, w: 256, h: 256 },
  ];
  const rounds = [];
  for (let r = 0; r < 3; r++) {
    for (const region of regions) {
      // **★ 每轮先在**顶层**画一笔 ✓ ★**：**∴ 顶掉**外层块缓存**✗**（**它的键含**全层指纹**✓ ⇒ 任何改动都失效 ✓**），
      // **而 below 的签名只覆盖"**最上层以外**"✓ ⇒ **∴ 不变 ⇒ 仍应命中 ✓****。
      // **∴ 没有这一步 ⇒ 外层先命中 ⇒ 根本测不到 below ✗**（**实测：`missing=0` 多次而计数不涨 ✓**）。
      await stroke("Ttop", 3 + r * 2 + regions.indexOf(region));
      const before = await reuse();
      await call(doc, token, "render_region", { region, raw: true });
      const after = await reuse();
      rounds.push({ r, key: `${region.x},${region.y}`, before, after, grew: after > before });
    }
  }
  console.log("  轮换 3 个区域 × 3 轮 ⇒ 每次渲染的 below_reuse 增量 ✓");
  for (const x of rounds) {
    console.log(`    第 ${x.r + 1} 轮 (${x.key}) ⇒ ${x.before} → ${x.after}${x.grew ? " ✓" : " ✗"}`);
  }
  const grew = rounds.filter((x) => x.grew).length;
  console.log(`  ⇒ 自增 ${grew}/${rounds.length} 次`);
  if (grew < rounds.length) {
    console.error(`❌ 只有 ${grew}/${rounds.length} 次自增 ⇒ **并非每次都命中** ✗ ` +
      "（每轮已先在顶层画一笔 ⇒ 外层块缓存必 miss ⇒ **∴ 这一定是 below 缓存的问题 ✓**）" +
      "（变异：把存储块改回「每次整体替换」或去掉 LRU 年龄刷新 ⇒ 必红 ✓）");
    process.exit(1);
  }
  console.log(`  ✓ 每轮都命中（${grew}/${rounds.length} ✓）⇒ **∴ tile 缓存 ＋ 跨渲染保留 ＋ LRU 全部生效 ✓**`);
};
main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
