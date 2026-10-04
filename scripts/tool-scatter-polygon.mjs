// **多边形散点区域判据**（测试报告 §二.3）：`area` 原来只认矩形 ⇒ 头发/衣服/斜投影会**溢出** ✗。
// 现在支持 `area.points = [[x,y], …]`。判据用**三角形**（斜边）做两处测量：
//   ① 三角形**内**必须有色 ✓（证明散点真的画了）；
//   ② 三角形**外、但仍在 bbox 内**的一角必须**完全无色** ✓（这一条才是判 §二.3 的）；
//   ③ 响应里的 `strokes` 是**真正落下的笔数** ✓（放不下会小于 count ✓，不许假装 ✓）。
// 能红：去掉"点在多边形内"的判定 ⇒ ② 立刻有色 ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-scatter-polygon.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "poly_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }) })).json()).token;

// 三角形 A(40,40) B(280,40) C(40,200) ⇒ 内部满足 2x + 3y <= 680（A: 200 ✓ 在内）。
const triangle = [[40, 40], [280, 40], [40, 200]];
const count = 40;
const scatter = await call(doc, token, "scatter_strokes", {
  layer_id: "layer_default", seed: 11, area: { points: triangle },
  palette: ["#C8BAA8"], brush: "100%_Opaque", count, size_range: [6, 9],
});
console.log("  scatter_strokes(area.points) ⇒ " + JSON.stringify(scatter).slice(0, 160));

const failures = [];
const placed = scatter && typeof scatter.strokes === "number" ? scatter.strokes : null;
if (placed === null) failures.push("响应里没有 strokes 计数 ⇒ 判据无法成立");
else if (placed > count) failures.push(`strokes=${placed} 大于 count=${count} ⇒ 计数不对`);
else if (placed === 0) failures.push("一笔都没落下 ⇒ 前置条件不成立");

const isBackground = (c) => c.r >= 245 && c.g >= 245 && c.b >= 245;
const inkOf = async (region, label) => {
  const stats = await call(doc, token, "analyze_region", { region });
  const ink = ((stats && stats.dominant_colors) || []).filter((c) => !isBackground(c));
  console.log(`  ${label} ⇒ 非背景主色 ${ink.length} 种${ink.length ? "：" + ink.slice(0, 2).map((c) => c.hex).join(",") : ""}`);
  return ink;
};
// ① 三角形内（重心附近）
const inside = await inkOf({ x: 60, y: 60, w: 60, h: 60 }, "三角形内(60,60,60x60)");
if (inside.length === 0) failures.push("三角形内没有笔触 ⇒ 散点没画上去（前置条件不成立）");
// ② 三角形外、bbox 内的一角：2*250+3*180=1040 > 680 ⇒ 必在外
const outside = await inkOf({ x: 230, y: 160, w: 50, h: 40 }, "三角形外(230,160,50x40)");
if (outside.length > 0) failures.push(`多边形外的区域出现了笔触（${outside.map((c) => c.hex).join(",")}）⇒ 溢出`);

if (failures.length) { console.log("  ✗ 多边形散点区域不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log(`  ✓ 多边形散点区域：内有着色、外无溢出（落下 ${placed}/${count} 笔）`);
process.exit(0);
