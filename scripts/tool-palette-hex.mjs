// **十六进制颜色判据**（测试报告第 1 条）：`scatter_strokes` 的 `palette` 说明写着支持 `#rrggbb`，
// 但底层只认 `{r,g,b,a}` ⇒ 十六进制串会**静默变成全黑**（r=g=b=0）。
// 判据两件事：① 传 `#C8BAA8` 落下的笔触**不能是黑的**、且应接近该色；
//            ② 传一个**根本无法解析**的颜色，必须**明确报错**（而不是又静默变黑）。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-palette-hex.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "hex_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 320, height: 240 }) })).json()).token;

const scatter = await call(doc, token, "scatter_strokes", {
  layer_id: "layer_default", seed: 7, area: { x: 20, y: 20, w: 200, h: 160 },
  palette: ["#C8BAA8"], brush: "100%_Opaque", count: 6, size_range: [8, 10],
});
console.log("  scatter_strokes ⇒ " + JSON.stringify(scatter).slice(0, 200));

// ① **量区域主色**（`list_objects` 不暴露颜色 ⇒ 用 analyze_region ✓）：
//    十六进制若被静默丢弃，主色里会出现一大块 #000000（实测 6.33% ⇔ 正确时应为 #c8baa8 ✓）。
const stats = await call(doc, token, "analyze_region", { region: { x: 20, y: 20, w: 200, h: 160 } });
const dom = (stats && stats.dominant_colors) || [];
console.log("  主色 ⇒ " + JSON.stringify(dom.slice(0, 4)));
const failures = [];
const isBackground = (c) => (c.r >= 245 && c.g >= 245 && c.b >= 245);
const ink = dom.filter((c) => !isBackground(c));
if (!dom.length) failures.push("analyze_region 没给出 dominant_colors ⇒ 判据无法成立");
else if (!ink.length) failures.push("区域里没有任何非背景色 ⇒ 散点根本没画上去");
else {
  const top = ink[0];
  const expect = { r: 0xc8, g: 0xba, b: 0xa8 };
  const off = Math.max(Math.abs(top.r - expect.r), Math.abs(top.g - expect.g), Math.abs(top.b - expect.b));
  if (off > 10) failures.push(`主色 ${top.hex} 与 #c8baa8 (200,186,168) 相差 ${off} ⇒ 十六进制没有被认出来`);
}

// ② 无法解析的颜色必须报错
const bogus = await call(doc, token, "scatter_strokes", {
  layer_id: "layer_default", seed: 7, area: { x: 20, y: 20, w: 60, h: 60 },
  palette: ["这不是颜色"], brush: "100%_Opaque", count: 2,
});
const bogusFailed = !bogus || bogus.ok === false || !!bogus.error;
console.log("  无法解析的颜色 ⇒ " + JSON.stringify(bogus).slice(0, 160));
if (!bogusFailed) failures.push("无法解析的颜色没有报错 ⇒ 又走回了静默路径");

if (failures.length) { console.log("  ✗ 十六进制调色板不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ 十六进制调色板：颜色正确落地，无法解析的值会报错");
process.exit(0);
