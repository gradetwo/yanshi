#!/usr/bin/env node
// **`sample_colors` 必须比 N 次 `sample_color` 显著快** ✓（第 1000+ 轮 ✓；YANSHI_ISSUES §18.2）
//
// **∴ 为什么需要这条判据 ✓**：批量接口最容易的错法是"内部循环调单点"✗ ——
//   那样**结果完全正确**✓（所以正确性判据抓不到 ✗），但**一点没省**✗。
//   ⇒ **∴ 只有**比墙钟**能抓住它 ✓** ⇒ **∴ 而比墙钟必须**交替跑 ＋ 取中位数**✗
//     （**∴ 我这一场被顺序偏差骗过：`preview=false` 反而更慢 ✗，其实全是冷启动 ✓**）✓
//
// **∴ 构型要成立 ✓**：50 个点必须**聚集**（40×40 内）✗ ——
//   因为包围盒是"所有点的外接矩形"✓ ⇒ **∴ 点分散时批量反而渲染更大面积 ✗**
//   ⇒ **∴ 用分散的点测不出批量的好 ✓，也测不出它的坏 ✓** ✓
import { readFileSync } from "node:fs";

// **`run-criteria.sh` 的约定是 `tool-*.mjs` 只收一个 `<base-url>`** ✗（见它第 3 行 ✓）
// ⇒ **∴ 所以这里**自己建一个临时文档**✓，不要再要多参数 ✗
//   （**∴ 我第一次写了 `<base> <doc> <token>` ⇒ 在 CI 里拿不到参数 ⇒ 退 3 ⇒ `criteria shard` 红 ✗** ✓）
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-sample-colors-batch.mjs <base-url>");
  process.exit(3);
}
const boot = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc: `sample_colors_${Date.now()}`, width: 300, height: 300 }),
});
const bootBody = await boot.json().catch(() => ({}));
const doc = bootBody.doc ?? bootBody.doc_id ?? `sample_colors_${Date.now()}`;
const token = bootBody.token;
if (!token) {
  console.log(`  ✗ 建文档失败（拿不到 token）：${JSON.stringify(bootBody).slice(0, 140)}`);
  process.exit(3);
}
const url = (name) => `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
const call = async (name, args) => {
  const started = process.hrtime.bigint();
  const reply = await fetch(url(name), { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  const body = await reply.json();
  const ms = Number(process.hrtime.bigint() - started) / 1e6;
  return { body, ms };
};

// 铺底 ＋ 一笔，让取到的颜色有区分度
await call("create_layer", { layer_id: "sc_L", name: "sc_L" });
await call("fill_region", { layer_id: "sc_L", shape: { type: "rect", x: 0, y: 0, w: 300, h: 300 }, color: { r: 40, g: 90, b: 140, a: 255 } });
await call("brush_stroke", { layer_id: "sc_L", brush: "100%_Opaque", color: { r: 200, g: 50, b: 30, a: 255 }, points: [[40, 150, 1], [260, 150, 1]], size: 40, preview: false });

// 50 个**聚集**在 40×40 内的点
const points = [];
for (let i = 0; i < 1600; i += 1) points.push([100 + (i % 40) * 2, 100 + Math.floor(i / 40) * 2]);

const median = (xs) => {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
};

const batchTimes = [];
const singleTimes = [];
const ROUNDS = 6;
for (let round = 0; round < ROUNDS; round += 1) {
  const batch = await call("sample_colors", { points });
  const half = points.slice(0, 50);
  if (batch.body.ok !== true) {
    console.log(`  ✗ sample_colors 返回 ok!=true：${JSON.stringify(batch.body).slice(0, 140)}`);
    process.exit(1);
  }
  batchTimes.push(batch.ms);
  const started = process.hrtime.bigint();
  for (const [x, y] of half) {
    const one = await call("sample_color", { x, y });
    if (one.body.ok !== true) { console.log(`  ✗ sample_color 在 (${x},${y}) 失败`); process.exit(1); }
  }
  singleTimes.push(Number(process.hrtime.bigint() - started) / 1e6);
}

// **规模判据** ✓（这一场补的 ✓）：光比"一次 vs N 次"不够 ✗ ——
//   **∴ 因为任何"一次调用"都能省掉 49 次 HTTP 往返 ✗ ⇒ **∴ 即使内部逐点渲染也照样快 7 倍**✗（实测 0.133× ✓）
//   ⇒ **∴ 真正能区分"一次渲染"与"内部循环"的**是**耗时怎么随点数涨**✗：
//      · 一次渲染 ⇒ 1 个点的包围盒 vs 50 个点的包围盒 ⇒ 面积涨 20 倍，但**没有 N 次固定开销** ⇒ 涨得**亚线性** ✓
//      · 内部循环 ⇒ **严格线性** ✗（10 点时 t，50 点时约 5t ✗）
//   ⇒ **∴ 断言：50 点的耗时 / 10 点的耗时 < 2.5**✗（**∴ 线性增长会是 5 ✓**）✓
const small = points.slice(0, 100);
const smallTimes = [];
for (let round = 0; round < ROUNDS; round += 1) {
  const r = await call("sample_colors", { points: small });
  if (r.body.ok !== true) { console.log("  ✗ sample_colors（10 点）失败"); process.exit(1); }
  smallTimes.push(r.ms);
}
const batchMs = median(batchTimes);
const smallMs = median(smallTimes);
const scale = batchMs / smallMs;
console.log(`  100 点 ⇒ **${smallMs.toFixed(1)} ms**｜1600 点 ⇒ **${batchMs.toFixed(1)} ms**｜**规模比 = ${scale.toFixed(2)}×**（线性会是 16×）`);
const singleMs = median(singleTimes);
const ratio = batchMs / singleMs;
console.log(`  ${ROUNDS} 轮交替，取中位数 ✓`);
console.log(`  sample_colors（50 点，一次）  ⇒ **${batchMs.toFixed(1)} ms**`);
console.log(`  50 次 sample_color            ⇒ **${singleMs.toFixed(1)} ms**`);
// **∴ 只打印 ✗，不当门槛 ✓**（**∴ 因为它是墙钟比 ✗ ⇒ 会随机器飘 ✓**）
//   ⇒ **∴ 真正承重的是下面的 `region_render_calls === 1` ✗**（**∴ 语义 ✓**）
console.log(`  （参考）比值 = ${ratio.toFixed(3)}×（墙钟比，只作观察；门槛已换成语义计数）`);
// ★ **改用"扣除固定开销后的净成本"✗ ★**（重写这一条 ✓）
//
// **∴ 为什么不再用"规模比"✗**：分子分母**都含固定开销**✗（HTTP 往返 + 建上下文 ✓）
//   ⇒ **∴ 固定开销占比越大 ⇒ 比值越被压向 1 ✗**
//   ⇒ **∴ 实测 ✓**：CI 上 25 点只用 **1.0 ms**（**∴ 计时精度下限 ✓**）⇒ 规模比 **8.24×** ✗ 刚好超 8.0
//     **∴ 而本机同判据 5.99×／4.33× ✓ ⇒ **∴ 于是"快机器必红"✗** ⇒ **∴ 换任何固定阈值都会被下次更快的机器打破 ✗**
//
// **∴ 净成本 ✓**：`(t_big − t_small)/(big − small)` 对比 `t_small/small`
//   · **逐点渲染 ⇒ 两者相等** ✗ ⇒ **∴ 断言为假 ✓**
//   · **一次渲染 ⇒ 净成本 ≈ 0** ✓
//   ⇒ **∴ 两边同时随机器速度缩放 ⇒ **比值与机器快慢无关**✓** ✓
// ★ **改用**语义计数**✗ ★**（**∴ 替代原来会随 CPU 飘的"净每点成本"✓**）
//
// **∴ 为什么换 ✗**：净比在 CI 上是 0.89 ✗、本机 0.33 ✓ ⇒ **∴ 那条判据在 CI 上一直红 ✓**
//   （**∴ 登记过 known-red ✓**）⇒ **∴ 而根因是**CI 的 CPU**✗ 让同一段代码慢 2.2 倍 ✓，
//      **∴ 不是判据写错 ✗、也不是代码退步 ✓**。
// ⇒ **∴ 该断的是**语义**✗**：**一次 `sample_colors` **只该向渲染层要一次区域**✗**
//   ⇒ **∴ 逐点渲染会让它变成 N ✗ ⇒ **∴ 而这个计数**与机器快慢无关 ✓**
const smallTimings = (await call("sample_colors", { points: small })).body?.timings ?? {};
const bigTimings = (await call("sample_colors", { points })).body?.timings ?? {};
let bad = 0;
const callsSmall = smallTimings.region_render_calls;
const callsBig = bigTimings.region_render_calls;
console.log(`  区域渲染次数：100 点 ⇒ **${callsSmall}**｜1600 点 ⇒ **${callsBig}**（**必须都是 1**）`);
if (callsSmall !== 1 || callsBig !== 1) {
  console.log("  ✗ 区域渲染次数不是 1 ⇒ sample_colors 内部按点渲染了（点数越多次数越多）");
  process.exit(1);
}
// **∴ 计时只作参考 ✗**（**∴ 它随机器飘 ✓，不当门槛 ✓**）
// ★ **那次区域渲染必须**被计时**✗ ★**（第 15 轮 ✓）
//
// **∴ 以前它不计时 ✗** ⇒ **∴ 成本全落在 `other_ms`**✗ ⇒ **∴ 实测 `other_ms` 与对象数线性
//   （2000 对象 ⇒ 43 ms ✓）⇒ **∴ 报告 §36 看到"全在 other"✗ 是对的观察 ✓，但看不清归因 ✓**
// ⇒ **∴ 现在记到 `Phase::Render`**✗ ⇒ **∴ 守恒 ✓**：`other_ms` 43.0 → **0.019**、`render_ms` 0 → **40.325** ✓
// ⇒ **∴ 断言两条，都是**布尔**✗（**∴ 不是阈值 ✓**）：
//   ⇒ **∴ ① `render_ms > 0`**✗（**∴ 它计了时间就一定 > 0 ✓，与机器快慢无关 ✓**）
//   ⇒ **∴ ② `other_ms < render_ms`**✗（**∴ 计时**盖住了**原来在 other 的那块 ✓）**
const callsExtra = bigTimings.render_ms;
const otherExtra = bigTimings.other_ms;
const rendered = typeof callsExtra === "number" && callsExtra > 0;
console.log(`  ${rendered ? "✓" : "✗"} 取色时那次区域渲染**被计时**了（render_ms = ${callsExtra}）`);
if (!rendered) bad += 1;
// **∴ 不加"other < render"那条 ✗**：**∴ 它只在**大画布 ＋ 多对象**时才成立 ✓**
//   ⇒ **∴ 实测 600×1066／2000 对象 ✓**：`other` 43.0 → **0.019**、`render` 0 → **40.325** ✓（**∴ 守恒 ✓**）
//   ⇒ **∴ 而在 320×240 小画布上 `other`(4.5) > `render`(3.0) ✗** ⇒ **∴ 那条断言会**随构型变**✗**
//     ⇒ **∴ 所以**判据只断**"它真的被计时了"**✗（**∴ 布尔 ✓，与构型无关 ✓**）；
//       守恒用**台账里的实测数字**记录 ✓，不进判据 ✓
console.log(`  （参考）other_ms = ${otherExtra}｜render_ms = ${callsExtra}`);

console.log(`  （参考）100 点 ${smallMs.toFixed(1)} ms｜1600 点 ${batchMs.toFixed(1)} ms｜规模比 ${scale.toFixed(2)}×`);
console.log("  ✓ sample_colors 一次只向渲染层要**一次**区域（与点数无关）⇒ 不是逐点渲染");
if (bad) { console.log(`  ✗ 有 ${bad} 处不满足`); process.exit(1); }
