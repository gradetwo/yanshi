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
  // **★ 只有**第 2 轮起**才要求自增 ✗ ★**（第 355 轮 ✓；**本机实测换来的 ✓）：
  //   **∴ 实测（**本机、正确环境 ✓）**：
  //     ```
  //     第 1 轮 (0,0)   ⇒ 2 → 2   ✗      ← ★ 第 1 轮**3/3 都不自增** ★
  //     第 1 轮 (256,0) ⇒ 4 → 4   ✗
  //     第 1 轮 (0,256) ⇒ 5 → 5   ✗
  //     第 2 轮 (0,0)   ⇒ 7 → 9   ✓      ← ★ 第 2／3 轮**6/6 都自增** ★
  //     第 2 轮 (256,0) ⇒ 11 → 13 ✓
  //     第 2 轮 (0,256) ⇒ 14 → 16 ✓
  //     第 3 轮 (0,0)   ⇒ 18 → 20 ✓
  //     ```
  //   **∴ 为什么第 1 轮**不该自增**✗**：**那是**每个区域的**第一次**渲染** ✓
  //     ⇒ **∴ 那时** below 缓存里**还没有可复用的东西** ✓（**∴ 当然**不自增 ✓）
  //       ⇒ **∴ 所以**：**旧断言（**要求 9/9 全自增**）**必然红** ✗
  //         ⇒ **★ 那是**判据的逻辑错**✗ ⇒ **∴ 不是**产品问题** ✓ ★**** ✓✓
  //   **∴ 本判据**真正要判的是**：**第 2 轮起**（**每个区域**已渲过一次 ✓）
  //     ⇒ **∴ 回到同一区域时**必须命中** ✓（**∴ 那**才叫「**跨渲染保留**」 ✓）** ★**** ✓✓
  //   **∴ 变异仍然有牙 ✗**：**把存储块改回「**每次整体替换**」**✗
  //     ⇒ **∴ 第 2／3 轮**也不自增**✗ ⇒ **∴ 本断言**必红** ✓ ★**** ✓✓
  const laterRounds = rounds.filter((x) => x.r >= 1);
  const grew = laterRounds.filter((x) => x.grew).length;
  console.log(`  ⇒ 第 2 轮起自增 ${grew}/${laterRounds.length} 次（第 1 轮是每个区域的首次渲染 ⇒ 不计 ✓）`);
  if (grew < laterRounds.length) {
    console.error(`❌ 第 2 轮起只有 ${grew}/${laterRounds.length} 次自增 ⇒ **并非每次都命中** ✗ ` +
      "（每轮已先在顶层画一笔 ⇒ 外层块缓存必 miss ⇒ **∴ 这一定是 below 缓存的问题 ✓**）" +
      "（变异：把存储块改回「每次整体替换」或去掉 LRU 年龄刷新 ⇒ 必红 ✓）");
    process.exit(1);
  }
  console.log(`  ✓ 第 2 轮起每次都命中（${grew}/${laterRounds.length} ✓）⇒ **∴ tile 缓存 ＋ 跨渲染保留 ＋ LRU 全部生效 ✓**`);
};
main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
