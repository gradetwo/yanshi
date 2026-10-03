#!/usr/bin/env node
// 笔刷名**放宽**的判据（真实回归报告：`marker-chisel` 报 reference_not_found、必须写 `Marker.myb` ✓）。
//
// 判据：同一个笔刷用**四种写法**都应当命中同一个 `.myb`；而**错名**的报错必须给出可行动的建议。
// 用法：node scripts/tool-brush-names.mjs <server-base>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-brush-names.mjs <server-base>"); process.exit(2); }
const post = async (path, payload) => (await fetch(base + path, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(payload || {}),
})).json();
const token = (await post("/api/documents", { doc_id: "bn", width: 300, height: 200 })).token;
if (!token) { console.error("拿不到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const tool = (name, args) => post("/api/tools?doc=bn&token=" + token, { tool: name, arguments: args });
await tool("create_layer", {});            // 判据不测默认图层，自己建一个，避免依赖别的行为
const names = await tool("list_assets", { kind: "brush" });
const listed = (names.assets || []).map((x) => (typeof x === "string" ? x : x.name)).filter((n) => n.endsWith(".myb"));
const sample = listed.find((n) => /^Marker\.myb$/i.test(n)) || listed[0];
if (!sample) { console.error("清单里没有 .myb ⇒ 判据无法运行"); process.exit(1); }
const stem = sample.replace(/\.myb$/, "");
let bad = 0;
const check = (label, ok, detail) => { console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : "")); if (!ok) bad += 1; };
for (const spelling of [sample, stem, stem.toLowerCase(), stem.toUpperCase()]) {
  const made = await tool("brush_stroke", { layer_id: "layer_1", brush: spelling, size: 30,
    points: [[10, 10, 1], [50, 10, 1]] });
  const detail = String((made.context || {}).detail || "");
  // 命中同一个笔刷 ⇒ 要么落笔成功，要么失败原因**不是**"找不到 brush"（例如图层问题）
  check("笔刷名可写成 " + JSON.stringify(spelling), !detail.includes("找不到 brush"), detail.slice(0, 60));
}
const wrong = await tool("brush_stroke", { layer_id: "layer_1", brush: stem.slice(0, 3) + "zzz", size: 30,
  points: [[10, 10, 1], [50, 10, 1]] });
const wrongDetail = String((wrong.context || {}).detail || "");
check("错名的报错要能指导下一步（含候选或清单指引）",
  wrongDetail.includes("你是不是要找") || wrongDetail.includes("list_assets"), wrongDetail.slice(0, 90));
console.log(bad ? "  结论：" + bad + " 条不成立 ✗" : "  结论：笔刷名放宽全部成立 ✓");
process.exit(bad ? 1 : 0);
