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

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-sample-colors-batch.mjs <base> <doc> <token>");
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
for (let i = 0; i < 50; i += 1) points.push([120 + (i % 10) * 4, 140 + Math.floor(i / 10) * 4]);

const median = (xs) => {
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
};

const batchTimes = [];
const singleTimes = [];
const ROUNDS = 6;
for (let round = 0; round < ROUNDS; round += 1) {
  const batch = await call("sample_colors", { points });
  if (batch.body.ok !== true) {
    console.log(`  ✗ sample_colors 返回 ok!=true：${JSON.stringify(batch.body).slice(0, 140)}`);
    process.exit(1);
  }
  batchTimes.push(batch.ms);
  const started = process.hrtime.bigint();
  for (const [x, y] of points) {
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
const small = points.slice(0, 10);
const smallTimes = [];
for (let round = 0; round < ROUNDS; round += 1) {
  const r = await call("sample_colors", { points: small });
  if (r.body.ok !== true) { console.log("  ✗ sample_colors（10 点）失败"); process.exit(1); }
  smallTimes.push(r.ms);
}
const batchMs = median(batchTimes);
const smallMs = median(smallTimes);
const scale = batchMs / smallMs;
console.log(`  10 点 ⇒ **${smallMs.toFixed(1)} ms**｜50 点 ⇒ **${batchMs.toFixed(1)} ms**｜**规模比 = ${scale.toFixed(2)}×**（线性会是 5×）`);
const singleMs = median(singleTimes);
const ratio = batchMs / singleMs;
console.log(`  ${ROUNDS} 轮交替，取中位数 ✓`);
console.log(`  sample_colors（50 点，一次）  ⇒ **${batchMs.toFixed(1)} ms**`);
console.log(`  50 次 sample_color            ⇒ **${singleMs.toFixed(1)} ms**`);
console.log(`  比值 = ${ratio.toFixed(3)}×（< 0.5 才算"真的省了" ✓）`);
if (scale >= 2.5) {
  console.log("  ✗ 耗时随点数线性增长（50点/10点 ≈ 5×）⇒ sample_colors 内部应该是逐点渲染了，等于没省");
  process.exit(1);
}
if (!(ratio < 0.5)) {
  console.log("  ✗ sample_colors 没有比逐点显著快 —— 最可能是内部循环调了单点（那样必然省不了）");
  process.exit(1);
}
console.log("  ✓ sample_colors 比 50 次 sample_color 快一倍以上，且耗时**亚线性**（不是逐点渲染）");
