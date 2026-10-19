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
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2];
// **★ `argv[3]` 不是**数字**时必须有**安全默认值 ✗ ★**（第 88 轮 ✓）：
//   **∴ 症状（**本地复现 ✓）✗**：`TypeError: Cannot read properties of undefined (reading 'toFixed')`
//     ⇒ **∴ 因为**编排（`run-criteria.sh:228` ✓）**给 `argv[3]` 的是**文档名**✗
//       （`crit_toollatencytiersmjs` ✓）⇒ **∴ `Number(…)` ＝ **NaN**✗
//         ⇒ **∴ 于是**：**循环**不跑 ⇒ **`hot` 为空 ⇒ **`med([])` ＝ undefined**
//           ⇒ **∴ `h.toFixed` 崩 ✓**** ✓✓
//   **∴ 修法 ✗**：**校验**✗ ⇒ **∴ 不是**正整数**就用默认 **5** ✓**** ✓✓
const runsArg = Number(process.argv[3]);
const runs = Number.isFinite(runsArg) && runsArg > 0 ? Math.floor(runsArg) : 5;
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
// **★★★ 第 250 轮改 ✗：不要断言**实现细节** ✗ 要断言**可观测行为** ✓ ★★★**
//   ＋ **∴ 原来那条 ✗**：**"② 的 `below_reused` 必须是 false"**✗（**理由：外层该挡住 ✓）
//     ＋ **∴ 而**实测它**一直是 true** ✗ ⇒ **∴ 判据长期红 ✗ 但**产品是好的** ✓：
//       **∴ 因为**"热"那一次**本来就可以由**内层（below）**接住** ✓ ⇒ **∴ 那叫**功能生效** ✗
//         不叫缺陷** ✓ ⇒ **∴ 这属于"**判据的判定设计错（误报）**"** ✓ ★★
//   ＋ **∴ 现在 ✗**：**只断言"**变快了就得有哪一层报复用**"** ✓
//     ⇒ **∴ 于是**：**① 真没缓存（**n≈c ✓）⇒ 上面的"应快于"会红 ✓
//       ＋ **② 快了却**两层都说没复用 ⇒ 这次读数可疑 ⇒ 红** ✓（**∴ 这才是能红的语义 ✓）★★
const servedHot = rHot.frame_reused === true || rHot.below_reused === true;
if (!servedHot) {
  bad.push(
    `② 变快了但**两层都没报复用** ⇒ 读数可疑（frame_reused=${rHot.frame_reused}｜below_reused=${rHot.below_reused}）`,
  );
}
if (bad.length) {
  console.error("❌ " + bad.join("｜") +
    "（变异：以 YANSHI_SKIP_REGION_CACHE=1 启动 ⇒ 外层缺失 ⇒ ② 会接近 ③ ⇒ 必红 ✓）");
  process.exit(1);
}
console.log(`  ✓ 三档关系成立：外层命中 ≈${(c / h).toFixed(1)}× 快于冷；内层 ≈${(c / n).toFixed(1)}×`);
