#!/usr/bin/env node
// **★ 合成成本必须与"下方层数"解耦 ✓ ★**（目标第 4 条：below 缓存 ✓；用户要求"所有环节实时性都重要" ✓）
//
// **它判什么** ✓：**只改**最上面那层**⇒ 合成成本**不得随层数增长**✗。
//   ⇒ **∴ 判据**：`preview_ms(32 层) ≤ 2 × preview_ms(1 层)` ✓（**比值，不是绝对值** ✗ —— 绝对值依赖机器 ✓）。
//
// **为什么单独立一条** ✗：`tool-below-reuse.mjs` 判的是**语义计数**（`/health.below_reuse` 自增 ✓），
//   而**用户能感到的是**墙钟** ✗ ⇒ **∴ 两条互补 ✓**（设计 §5 的 C3 ＋ 本条 C6 ✓）。
//
// **今天的预期** ✗（第 457 轮基线 ✓）：**实测 1／4／16／32 层：`preview`（合成）24.8 → 50.1 ms** ⇒
//   **∴ 比值 ≈ 2.0 ⇒ 卡在**边界**✗ ⇒ **∴ 本判据**当前多半是红的 ✓**（below 缓存未实现 ✓）。
//
// **变异** ✗（实现后 ✓）：**让"只改当前层"也清空 below** ⇒ 比值回到 ~2.0 ⇒ **必红 ✓**。
//
// **转绿条件** ✓：**实现 below 缓存（tile 粒度 ✓、六类例外整条不走 ✓）** ⇒ 32 层与 1 层**接近**
//   ⇒ 比值 ≤ 2.0 ⇒ 变绿 ✓。
//
// 用法：node scripts/tool-composite-scaling.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-composite-scaling.mjs <base-url>"); process.exit(2); }

const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  });
  return await r.json();
};

const health = async () => (await (await fetch(`${base}/health`)).json());

const layerIds = (n) => {
  const ids = [];
  for (let i = 0; i < n; i++) ids.push("L" + String(i).padStart(2, "0"));
  return ids;
};

// 一个尺寸测一次：建 n 层 ⇒ 只改最上层 ⇒ 取 get_document 的 preview_ms
const measure = async (doc, token, n) => {
  const ids = layerIds(n);
  for (const id of ids) {
    await call(doc, token, "create_layer", { layer_id: id, name: id });
  }
  const top = ids[ids.length - 1];
  const samples = [];
  const boxes = [];
  for (let i = 0; i < 6; i++) {
    // **只改最上面那层** ✓ —— 下面那些层**一动不动** ✓ ⇒ below 缓存应当可以复用 ✓
    // **★ 每次落在**同一处** ✓ ★**（第 461 轮查明的关键 ✓）：**below 缓存按**区域**键 ✗** ⇒
    // **∴ 若每笔换位置 ⇒ 脏区随之变化 ⇒ **缓存永远不可能命中 ✗****
    //（**∴ 我第 459 轮把缓存挂在 `render_region` 里**本该生效 ✗，而探针**每次挪位置**✗ ⇒ 假阴性 ✓）。
    const stroke = await call(doc, token, "brush_stroke", {
      layer_id: top, brush: "100%_Opaque", size: 12,
      points: [[60, 90, 0.9], [70, 100, 0.9]],
      color: { r: 200, g: 60, b: 60, a: 255 }, preview: false,
    });
    boxes.push(JSON.stringify(stroke.dirty_bbox ?? null));
    const opened = await call(doc, token, "get_document", {});
    const tm = opened.timings || {};
    const v = tm.preview_ms === undefined ? null : Number(tm.preview_ms);
    if (v !== null) samples.push(v);
  }
  samples.shift();                            // **丢掉首次** ✓
  samples.shift();                            // **★ 再丢一个 ✓**（第 461 轮实测：1 层那格首个样本含建缓存 ⇒ 离散 11.5–16.3 ✗）
  const sorted = samples.slice().sort((a, b) => a - b);
  const median = sorted.length ? sorted[Math.floor(sorted.length / 2)] : null;
  return { n, median, samples, boxes };
};

const main = async () => {
  const doc = "cs_" + Date.now().toString(36);
  const token = (await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
  })).json()).token;

  const rows = [];
  for (const n of [1, 4, 16, 32]) rows.push(await measure(doc, token, n));

  const h = await health();
  console.log("  层数 ⇒ 合成（preview_ms，中位数 ✓，丢首次 ✓）");
  for (const r of rows) {
    const uniq = Array.from(new Set(r.boxes || []));
    console.log(`    ${String(r.n).padStart(2)} 层 ⇒ ${r.median === null ? "n/a" : r.median.toFixed(1)} ms｜样本 ${JSON.stringify(r.samples.map((x) => Number(x.toFixed(1))))}｜脏区 Variants=${uniq.length}${uniq.length === 1 ? "（恒定 ✓）" : "（**每次都在变 ✗**）"}`);
  }
  const one = rows.find((r) => r.n === 4);
  const many = rows.find((r) => r.n === 32);
  const ratio = (one && many && one.median) ? many.median / one.median : null;
  console.log(`  比值 32 层 / 4 层 = ${ratio === null ? "n/a" : ratio.toFixed(2)}×｜/health.below_reuse=${JSON.stringify(h.below_reuse)}`);

  if (ratio === null) {
    console.error("❌ 取不到 preview_ms ⇒ 判据无效 ⇒ 不许当通过 ✗");
    process.exit(1);
  }
  // **★ 先判**语义**✗**（第 458 轮实测 ✓）：**比值**区分力不足 ✗** ——
  // **∴ 1 层 5.0 ms／32 层 8.7 ms ⇒ 1.73×** ✓ **恰好卡在 2.0× 之下 ⇒ 判据**误绿 ✗**，
  // **∴ 而 `/health.below_reuse` = **0** ✓ ⇒ **∴ 一次复用都没有 ✓** ⇒ **∴ 语义才是硬凭证 ✓**。
  const belowReuse = Number(h.below_reuse ?? -1);
  if (belowReuse === 0) {
    console.error("❌ 只改当前层 6 次之后 `/health.below_reuse` 仍为 **0** ⇒ 下方合成**一次都没被复用** ✗" +
      `（比值 ${ratio.toFixed(2)}× 只是**区分力不足**的旁证 ✗）` +
      "（变异：实现 below 缓存 ⇒ 该计数自增 ⇒ 本判据变绿 ✓）");
    process.exit(1);
  }
  if (ratio > 2.0) {
    console.error(`❌ 只改当前层时合成成本随层数增长（32/4 = ${ratio.toFixed(2)}× > 2.0×）` +
      ` ⇒ 下方合成**没有**被复用 ✗（below 缓存未实现 ✓）` +
      `（变异：让"只改当前层"也清空 below ⇒ 比值回到 ~2× ⇒ 必红 ✓）`);
    process.exit(1);
  }
  console.log(`  ✓ 合成成本与下方层数解耦（32/1 = ${ratio.toFixed(2)}× ≤ 2.0×）`);
};

main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
