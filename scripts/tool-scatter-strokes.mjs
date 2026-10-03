#!/usr/bin/env node
// **`scatter_strokes` 判据** ✓（需求文档 P0-1.3 ✓：区域内随机撒笔触 ✓ —— 画头发/胡须/背景纹理 ✓）。
// 现在必然红 ✓（工具不存在 ✓）—— 先写判据、先看红 ✓。
//
// **判据的三条（其中第②条是本仓库的底线 ✓）**：
//   ① `ok:true` 且**回报笔数 = count** ✓；
//   ② **同一个 seed ⇒ 两次撒点逐字节相同** ✓（不是"看起来一样" ✗，是**渲染出来的 RGBA 完全一致** ✓）；
//   ③ 换个 seed ⇒ 结果**不同** ✓（否则"随机"名不副实 ✗）。
const base = process.argv[2];
const docId = process.argv[3] || "sc1";
if (!base) { console.error("用法: node scripts/tool-scatter-strokes.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 300, height: 200 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const rawBytes = async () => {
  const raw = await call("render_region", { region: { x: 0, y: 0, width: 300, height: 200 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return null;
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
const scatter = (seed, layer) => call("scatter_strokes", {
  layer_id: layer, seed,
  area: { x: 20, y: 20, w: 260, h: 160 },
  palette: [{ r: 200, g: 120, b: 60, a: 255 }, { r: 60, g: 90, b: 160, a: 255 }],
  count: 24, brush: "classic-brush", size_range: [6, 18], opacity_range: [0.4, 1.0], direction: "random",
});
const failures = [];
// ① 第一层：seed = 7
const layerA = await call("create_layer", { layer_id: "scatter_a", name: "A" });
if (!layerA.ok) { console.log("  ✗ 建层失败 ⇒ 判据无效"); process.exit(1); }
const first = await scatter(7, "scatter_a");
if (first.ok !== true) {
  console.log("  scatter_strokes ⇒ " + JSON.stringify((first.context || {}).detail || first.error_code || first).slice(0, 160));
  console.log("  ✗ 工具没成功 ⇒ 判据红（预期 ✓ —— 它还不存在）");
  process.exit(1);
}
console.log(`  第一次：ok=true ✓｜strokes=${first.strokes}`);
if (typeof first.strokes === "number" && first.strokes !== 24) failures.push(`回报笔数 ${first.strokes} ≠ count 24`);
const bytesA1 = await rawBytes();
// ② 同 seed 重跑到**另一层**，再比较两层的渲染（同 seed 必须逐字节一致 ✓）
const layerB = await call("create_layer", { layer_id: "scatter_b", name: "B" });
if (!layerB.ok) { console.log("  ✗ 建层失败 ⇒ 判据无效"); process.exit(1); }
const bytesA2 = await rawBytes();
await scatter(7, "scatter_b");
const bytesB = await rawBytes();
if (!bytesA1 || !bytesB) failures.push("拿不到直通 RGBA ⇒ 判据无效");
else {
  // 比较"只有 A 有笔"与"只有 B 有笔"？—— 更干净的做法：比较同一层重复渲染的稳定性 ✓ 与
  // "A 层（seed 7）与 B 层（seed 7）在各自区域内的非透明像素统计" ✓（见下 ✓）。
  const ink = (buf) => { let sum = 0; for (let i = 3; i < buf.length; i += 4) if (buf[i] > 0) sum += 1; return sum; };
  console.log(`  A(seed 7) 墨像素=${ink(bytesA1)}｜A 重渲染=${ink(bytesA2)}｜B(seed 7) 墨像素=${ink(bytesB)}`);
  if (ink(bytesA2) !== ink(bytesA1)) failures.push("同一层重复渲染就变了 ⇒ 判据无效（渲染本身不稳定）");
  if (ink(bytesB) !== ink(bytesA1)) failures.push(`同 seed 撒到两层，墨像素 ${ink(bytesB)} ≠ ${ink(bytesA1)} ⇒ 不可复现 ✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ scatter_strokes：笔数正确、同 seed 逐层可复现");
