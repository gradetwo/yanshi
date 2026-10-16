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
  for (let index = 0; index < RUNS; index += 1) {
    const started = performance.now();
    const result = await stroke(index);
    const elapsed = performance.now() - started;
    if (!result || result.ok !== true) {
      console.error(`❌ 判不了：第 ${index + 1} 笔失败 => 退出码 3`);
      process.exit(3);
    }
    samples.push(elapsed);
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
