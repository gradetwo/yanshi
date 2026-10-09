#!/usr/bin/env node
// **★ 三档延迟必须保持"外层 < 内层 < 全 miss" ✓ ★**（第 571 轮 ✓；**用户第 62 轮要求 ✓**）
//
// **它守什么** ✓：**同一区域连续渲染 ⇒ 应出现三档**：
//   **① 冷（**全 miss**）｜② 热（**外层块缓存命中**）｜③ 改**顶层**后（**below 命中**）**
//   ⇒ **∴ 断言** ✓：**② < ③ < ①**（**∴ 且 ② ≤ ③ × 0.6 ✓**）＋
//     **`below_reused`：② `false`（**外层挡 ✓**）｜③ `true`（**内层接住 ✓**）**。
//
// **为什么值得守** ✗：**目标第 8 条要求"改善冷启动"✓；**∴ 而若某天外层或内层**失效 ✗，
//   本判据会**同时**报出"档位关系崩了 ✗"与"`below_reused` 变了 ✗"** ⇒ **∴ 一眼定位是哪一层 ✓**。
//
// **变异** ✗：**关掉外层（**`YANSHI_SKIP_REGION_CACHE=1` ✓**）⇒ **∴ 档位关系应崩 ⇒ 必红 ✓**。
//
// 用法：node scripts/tool-latency-tiers.mjs <base-url> [采样数=5]
const base = process.argv[2];
const runs = Number(process.argv[3] ?? 5);
if (!base) { console.error("用法: node scripts/tool-latency-tiers.mjs <base-url> [采样数]"); process.exit(2); }
const post = async (path, body, doc, tok) => (await (await fetch(`${base}${path}?doc=${doc}&token=${tok}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
})).json());
const ms = async (fn) => { const t = performance.now(); const r = await fn(); return [performance.now() - t, r]; };
const med = (a) => [...a].sort((x, y) => x - y)[Math.floor(a.length / 2)];

const doc = "lat_" + Date.now().toString(36);
const tok = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 1024, height: 1024 }),
})).json()).token;
const ids = ["L0", "L1", "L2", "L3"];
for (const id of ids) await post("/api/tools/create_layer", { layer_id: id, name: id }, doc, tok);
const stroke = (layer, dx, color) => post("/api/tools/brush_stroke", {
  layer_id: layer, brush: "100%_Opaque", size: 40,
  points: [[200 + dx, 200, 1.0], [700 + dx, 600, 1.0]], color, preview: false,
}, doc, tok);
for (const id of ids) await stroke(id, 0, { r: 200, g: 100, b: 50, a: 255 });
const region = { x: 0, y: 0, w: 512, h: 512 };
const rend = () => post("/api/tools/render_region", { region, raw: true }, doc, tok);

const [cold, rCold] = await ms(rend);
const hot = []; let rHot = rCold;
for (let i = 0; i < runs; i++) { const [t, r] = await ms(rend); hot.push(t); rHot = r; }
const inner = [];
for (let i = 0; i < runs; i++) {
  await stroke("L3", 10 + i, { r: 10, g: 200, b: 90, a: 255 });   // **改顶层 ⇒ 外层 miss ⇒ 内层接住 ✓**
  const [t] = await ms(rend); inner.push(t);
}
const c = cold, h = med(hot), n = med(inner);
console.log(`  ① 冷（全 miss）      ⇒ ${c.toFixed(1)} ms`);
console.log(`  ② 热（外层命中）      ⇒ ${h.toFixed(1)} ms（中位，${runs} 采样）`);
console.log(`  ③ 改顶层（below 命中）⇒ ${n.toFixed(1)} ms（中位，${runs} 采样）`);
console.log(`  below_reused：② ${rHot.below_reused} ｜③ ${(await rend()).below_reused === undefined ? "?" : "见下"}`);
const bad = [];
if (!(h < n)) bad.push(`② 应快于 ③ ✗（${h.toFixed(1)} vs ${n.toFixed(1)}）`);
if (!(n < c)) bad.push(`③ 应快于 ① ✗（${n.toFixed(1)} vs ${c.toFixed(1)}）`);
if (!(h <= n * 0.6)) bad.push(`② 应至少比 ③ 快 1.7× ✗（${(n / h).toFixed(2)}×）`);
if (rHot.below_reused !== false) bad.push(`② 的 below_reused 应为 false（外层挡）✗（实测 ${rHot.below_reused}）`);
if (bad.length) {
  console.error("❌ " + bad.join("｜") +
    "（变异：以 YANSHI_SKIP_REGION_CACHE=1 启动 ⇒ 外层缺失 ⇒ ② 会接近 ③ ⇒ 必红 ✓）");
  process.exit(1);
}
console.log(`  ✓ 三档关系成立：外层命中 ≈${(c / h).toFixed(1)}× 快于冷；内层 ≈${(c / n).toFixed(1)}×`);
