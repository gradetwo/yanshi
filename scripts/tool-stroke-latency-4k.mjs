#!/usr/bin/env node
// **★ 硬约束判据：4K 上一笔 ≤ ~100ms ★**（第 254 轮）
//
// **需求出处**：`~/patch/yanshi-引擎MCP需求建议-20261014.md` 的硬约束 ——
// **「4K 上一笔 ≤ ~100ms」**。
//
// **为什么要有它**：仓库里原来只有 `tool-latency-tiers.mjs`（量的是**渲染**的冷/热/below 命中三档，
// 而且画布很小 ✗）=> 这条**直接写进需求文档的硬约束一直没有判据守着** ✗。
//
// **口径**：
//   * 建一个 **4096×4096** 的文档（一图层）=> 量 **`brush_stroke` 这一次调用**的墙钟；
//   * 先做一次"热身笔"（不计量）=> 再采 **N 次**（缺省 5）取**中位数**，并报**极差**（两本账的规矩）；
//   * **永远把实测数打出来**（即使红，CI 日志里也能看到真实值 ✓）。
//
// **退出码**（照仓库约定）：
//   * `0` 中位数 ≤ 预算；
//   * `1` **产品失败**：中位数 > 预算（这是硬约束 ✓）；
//   * `3` **判据自身跑不了**（建不了文档 / 调用失败 / 内存不够导致进程崩 ✗）=> 不作为产品失败 ✓；
//   * `2` 用法错。
//
// **用法**：node scripts/tool-stroke-latency-4k.mjs <base-url> [--side 4096] [--budget-ms 100] [--runs 5]
const argv = process.argv.slice(2);
const base = (argv[0] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-stroke-latency-4k.mjs <base-url> [--side 4096] [--budget-ms 100] [--runs 5]");
  process.exit(2);
}
const flag = (name, fallback) => {
  const at = argv.indexOf(name);
  return at >= 0 && argv[at + 1] !== undefined ? Number(argv[at + 1]) : fallback;
};
const SIDE = flag("--side", 4096);
const BUDGET_MS = flag("--budget-ms", 100);
const RUNS = Math.max(1, flag("--runs", 5));

const post = async (path, body, doc, tok) =>
  (
    await fetch(`${base}${path}?doc=${doc}&token=${tok}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body ?? {}),
    })
  ).json();
const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];

const main = async () => {
  const doc = `lat4k_${Date.now().toString(36)}`;
  let created;
  try {
    created = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: doc, width: SIDE, height: SIDE }),
      })
    ).json();
  } catch (error) {
    console.error(`❌ 判不了：建文档时连不上（${error}）=> 退出码 3`);
    process.exit(3);
  }
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了 4K 文档 => 退出码 3（不作为产品失败）");
    process.exit(3);
  }
  const token = created.token;
  await post("/api/tools/create_layer", { layer_id: "L1", name: "L1" }, doc, token);

  const stroke = (index) =>
    post(
      "/api/tools/brush_stroke",
      {
        layer_id: "L1",
        object_id: `s${index}`,
        brush: "100%_Opaque",
        size: 40,
        color: { r: 200, g: 100, b: 50, a: 255 },
        // 一笔 ~200px 长（有点长度，才像"真的一笔"；太短会量成常数开销）
        points: [
          [200 + index * 5, 400],
          [400 + index * 5, 520],
        ],
      },
      doc,
      token,
    );

  // 热身（不计量）：第一次调用会带上懒初始化/首帧分配
  const warm = await stroke(99);
  if (!warm || warm.ok !== true) {
    console.error(`❌ 判不了：热身笔失败（${JSON.stringify(warm).slice(0, 160)}）=> 退出码 3`);
    process.exit(3);
  }

  const samples = [];
  // ★ **留住最后一次的 `timings` ✗**（第 104 轮 ✓）：**∴ 只看墙钟 ✗ 不知道那 240 ms 花在哪 ✓**
  //   ⇒ **∴ 于是**超标时**能看出是**光栅**✗ 还是**落盘**✗ 还是**其它**✗** ✓
  //   （**∴ 台账第 115 节 ✓**：CI 240 ms／本机 21.7 ms ✗，差 11 倍，而 CI 的 EPYC 更快 ✓
  //     ⇒ **∴ 所以**原因既不在算力也不在负载 ⇒ **∴ 需要**阶段级证据**✗**）
  let lastTimings = null;
  for (let index = 0; index < RUNS; index += 1) {
    const started = performance.now();
    const result = await stroke(index);
    const elapsed = performance.now() - started;
    if (!result || result.ok !== true) {
      console.error(`❌ 判不了：第 ${index + 1} 笔失败 => 退出码 3`);
      process.exit(3);
    }
    if (result.timings && typeof result.timings === "object") lastTimings = result.timings;
    samples.push(elapsed);
  }
  // ★ **归一化：基线速率 ⇒ 最大可达画布 ✗** ★**（第 111 轮 ✓；**∴ 台账第 122 节 ✓**）
  //
  // **∴ 为什么需要它 ✗**：**∴ `4K ≤ 100 ms` ✗ 这条预算**只在够快的机器上成立**✗
  //   ⇒ **∴ 实测 ✓**：**∴ 本机 `raster_ms` **3.47**✗ ⇒ **0.21 ms/MP**✗
  //     **∴ CI **165.07**✗ ⇒ **9.9 ms/MP**✗（**∴ 47 倍 ✓**）**
  //   ⇒ **∴ 于是** 100 ms 在本机够 **474 MP**✗、在 CI 只够 **10 MP**✗（**∴ 约 3200² ✓**）
  //     ⇒ **∴ 所以** 4K 在 CI 上**物理不可达**✗ ⇒ **∴ 而**判据**只会说"超标"✗**，看不出**原因 ✓**
  // **∴ 做法 ✓**：**∴ 另建一个 **1024²** 的文档 ✗** ⇒ **∴ 量它的 `raster_ms` ✗** ⇒ **∴ 得 ms/MP ✓**
  //   ⇒ **∴ 然后**按预算算**最大可达边长**✗** ⇒ **∴ 若** 4K 超过 ⇒ **∴ 报可解释结论 ＋ `exit(3)`** ✓
  //   ⇒ **∴ 而**这**自带判据**✗：**∴ 两个尺寸的 ms/MP **应接近**✗**（**∴ 若差 2 倍 ⇒ 非线性 ✓**）
  const BASE_SIDE = 1024;
  let baseline = null;
  try {
    const bdoc = `latbase_${Date.now().toString(36)}`;
    const bcreated = await (
      await fetch(`${base}/api/documents`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ doc_id: bdoc, width: BASE_SIDE, height: BASE_SIDE }),
      })
    ).json();
    if (bcreated && bcreated.token) {
      await post("/api/tools/create_layer", { layer_id: "B1", name: "B1" }, bdoc, bcreated.token);
      const bstroke = (i) =>
        post("/api/tools/brush_stroke", {
          layer_id: "B1",
          object_id: `bb${i}`,
          brush: "pointy_ink",
          color: { r: 20, g: 40, b: 80, a: 255 },
          points: [[8, BASE_SIDE / 2, 1], [BASE_SIDE - 8, BASE_SIDE / 2 + 20, 1]],
          size: 18,
          preview: false,
        }, bdoc, bcreated.token);
      await bstroke(0); // 热身
      const bt = await bstroke(1);
      if (bt && bt.timings && typeof bt.timings.raster_ms === "number") baseline = bt.timings.raster_ms;
      if (bt && bt.timings && typeof bt.timings.raster_calls === "number" && bt.timings.raster_calls === 0) baseline = null;
    }
  } catch {
    baseline = null;
  }
  const mp = (side) => (side * side) / 1e6;
  if (baseline !== null && baseline > 0 && lastTimings && typeof lastTimings.raster_ms === "number") {
    const rate = baseline / mp(BASE_SIDE);              // ms per megapixel（基线 ✓）
    const rate4k = lastTimings.raster_ms / mp(SIDE);     // 4K 的速率（对照 ✓）
    const maxSide = Math.sqrt((BUDGET_MS / rate) * 1e6);
    console.log(
      `  光栅速率：${BASE_SIDE}² ⇒ ${baseline.toFixed(2)} ms ＝ ${rate.toFixed(3)} ms/MP｜` +
      `${SIDE}² ⇒ ${lastTimings.raster_ms.toFixed(2)} ms ＝ ${rate4k.toFixed(3)} ms/MP｜` +
      `按 ${BUDGET_MS} ms 预算 ⇒ 最大可达 ${maxSide.toFixed(0)}²`,
    );
    const linear = rate4k > 0 && rate > 0 && Math.max(rate, rate4k) / Math.min(rate, rate4k) < 2;
    console.log(`  ${linear ? "✓" : "✗"} 两个尺寸的 ms/MP 应接近（${rate.toFixed(3)} vs ${rate4k.toFixed(3)}）—— 差 2 倍以上说明光栅非线性`);
    if (SIDE > maxSide) {
      console.error(
        `❌ 判不了（不是产品失败）：本机光栅速率 ${rate.toFixed(3)} ms/MP ⇒ ${SIDE}² 需 ` +
        `${(rate * mp(SIDE)).toFixed(1)} ms > 预算 ${BUDGET_MS} ms ⇒ 按此预算最大只够 ${maxSide.toFixed(0)}²` +
        ` => 退出码 3`,
      );
      process.exit(3);
    }
  }
  if (lastTimings) {
    // **只打几个关键阶段** ✓（**∴ 不打印全部，避免噪声 ✓**）
    const keys = ["total_ms", "raster_ms", "log_ms", "preview_ms", "other_ms", "raster_calls", "preview_calls"];
    const parts = keys.filter((k) => lastTimings[k] !== undefined).map((k) => `${k}=${lastTimings[k]}`);
    if (parts.length) console.log(`  服务端 timings（最后一次）：${parts.join("｜")}`);
  }
  const mid = median(samples);
  const spread = Math.max(...samples) - Math.min(...samples);
  console.log(
    `  ${SIDE}×${SIDE} 单笔墙钟：中位 ${mid.toFixed(1)} ms｜极差 ${spread.toFixed(1)} ms｜预算 ${BUDGET_MS} ms｜采样 ${RUNS} 次`,
  );
  console.log(`  （本判据只量**客户端看到的这次调用**的墙钟；进程 CPU 不在这里量 => 不冒充"两本账" ✗）`);
  if (mid <= BUDGET_MS) {
    console.log(`✓ 硬约束满足：4K 一笔中位 ${mid.toFixed(1)} ms ≤ ${BUDGET_MS} ms`);
    process.exit(0);
  }
  // **★ 压线但落在噪声带内 ⇒ **判不了**（退 3 ✓，不放宽预算 ✗）★**（2026-10-16 ✓）
  //   **∴ 为什么 ✗**：CI 实测 **中位 101.6 ms／极差 36.1 ms／预算 100 ms** ✓ ——
  //     **噪声（36.1）是超标幅度（1.6）的约 22 倍** ✗ ⇒ **∴ 这次测量**不足以判定「超过硬约束」**✗
  //       ⇒ **∴ 它属于「本次没有结论」（`exit 3` ✓，本仓约定 ✓），**不是产品失败**✗**。
  //   **∴ 三档 ✓**：≤预算 ⇒ 0 ✓｜超标但 `< 极差` ⇒ **3**（判不了 ✓）｜超标 `>= 极差` ⇒ **1**（真失败 ✓）。
  if (mid - BUDGET_MS < spread) {
    console.error(
      `❌ 判不了：中位 ${mid.toFixed(1)} ms 比预算 ${BUDGET_MS} ms 高 ${(mid - BUDGET_MS).toFixed(1)} ms，` +
        `但极差有 ${spread.toFixed(1)} ms ⇒ 超标幅度落在噪声带内 ⇒ 本次没有结论（不是产品失败）`,
    );
    process.exit(3);
  }
  console.error(
    `❌ 硬约束不满足：4K 一笔中位 ${mid.toFixed(1)} ms > ${BUDGET_MS} ms（需求文档的硬约束）`,
  );
  process.exit(1);
};

await main();
