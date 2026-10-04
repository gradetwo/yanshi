#!/usr/bin/env node
// **「选区外零像素变化」判据** ✓（需求文档 P0-3 选区/蒙版 ✓）。
// 文档给的理由很具体 ✓："想改脸，笔会画到背景上，只能靠坐标小心避开"✗。
//
// **先查了现状** ✓：`create_selection` / `delete_selection` / `list_selections` **都已有** ✓，
// 但落笔路径里**只见画布越界、不见选区** ✗（`brush.rs` 里那个 `coverage(...)` 掩码更像**图层蒙版** ✓）
// ⇒ 所以这条判据问的是："**建了选区之后，落笔真的会被裁住吗**" ✓。
//
// 判据（半边选区 + 一条横跨两半的笔触 ⇒ 答案确定 ✓）：
//   ① 建一个只盖**左半**的矩形选区 ✓；
//   ② 画一条**从最左到最右**的笔触 ✓；
//   ③ **左半必须变** ✓（选区内的墨要落下 ✓）；
//   ④ **右半必须一个字节都不变** ✓（选区外不许有墨 ✗）—— 这一条就是需求的核心 ✓。
const base = process.argv[2];
const docId = process.argv[3] || "sel1";
if (!base) { console.error("用法: node scripts/tool-selection-clip.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 200, height: 100 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const shot = async (x, w) => {
  const raw = await call("render_region", { region: { x, y: 0, w: w, h: 100 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return null;
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
await call("create_layer", { layer_id: "layer_default", name: "l" });
const selection = await call("create_selection", { selection_id: "sel_judge", shape: { kind: "rect", bbox: { x: 0, y: 0, w: 100, h: 100 } } });
console.log("  create_selection ⇒ ok=" + selection.ok + "｜" + JSON.stringify(selection).slice(0, 140));
const leftBefore = await shot(10, 80);
const rightBefore = await shot(120, 80);
const painted = await call("brush_stroke", {
  layer_id: "layer_default", brush: "classic-brush",
  points: [[10, 50, 0.6], [190, 50, 0.6]], size: 18, color: { r: 30, g: 60, b: 200, a: 255 },
  // **显式裁剪** ✓：需求文档的接口就是 `brush_stroke(..., clip_to_selection)` ✓ ——
  // 我上一版假设"建了选区就自动裁" ✗，那是**我自己的**设计选择 ✓，不是需求 ✓。
  clip_to_selection: "sel_judge",
});
if (painted.ok !== true) { console.log("  ✗ 落笔失败 ⇒ 判据无效 ⇒ " + JSON.stringify(painted.context || painted.error_code)); process.exit(1); }
const leftAfter = await shot(10, 80);
const rightAfter = await shot(120, 80);
if (!leftBefore || !rightBefore || !leftAfter || !rightAfter) { console.log("  ✗ 取不到像素 ⇒ 判据无效"); process.exit(1); }
const changed = (a, b) => a.reduce((n, v, i) => (v !== b[i] ? n + 1 : n), 0);
const leftDiff = changed(leftBefore, leftAfter);
const rightDiff = changed(rightBefore, rightAfter);
console.log(`  选区内（左半）变化字节 = ${leftDiff}（应 > 0 ✓）`);
console.log(`  选区外（右半）变化字节 = ${rightDiff}（**必须为 0** ✓）`);
const failures = [];
if (selection.ok !== true) {
  console.log("  ✗ 建选区就失败 ⇒ 判据无效 ⇒ " + JSON.stringify((selection.context || {}).detail || selection.error_code));
  process.exit(1);
}
if (leftDiff === 0) failures.push("选区内也没落墨 ⇒ 判据无效或选区把整笔都挡了 ✗");
if (rightDiff !== 0) failures.push(`选区外落了 ${rightDiff} 字节的墨 ⇒ 落笔没有认选区 ✗（需求的核心正是这一条）`);
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 选区：内落墨、外零字节 ⇒ 落笔认选区");
