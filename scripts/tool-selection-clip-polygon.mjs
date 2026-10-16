#!/usr/bin/env node
// **★ 纯点列多边形也要能裁剪 ✗ ★**（2026-10-16 ✓；**先复现、再读代码、再修** ✓）
//
// **∴ 病 ✗**（本机实测 ＋ 读到 `tools.rs:16074` ✓）：造"可裁剪选区"名单时用的是
//   `filter_map(|s| { let bbox = s.shape.get("bbox")?; … })` ✓ ⇒ **没有 `bbox` 的选区被静默丢掉** ✗
//   ⇒ 而**纯点列多边形**（`{kind:"polygon", points:[…]}` ✓）**本来就没有 bbox** ✗
//   ⇒ 名单为空 ⇒ `brush_stroke(clip_to_selection)` 报
//     「**找不到可裁剪的选区 poly ⇒ 现有：（一个都没有 ✓）**」✗
//   **∴ 而 `create_selection` 回的是 `ok=true`** ✓、选区**确实在** `state.selections` 里 ✓
//   ⇒ **∴ 又是"回执没错、下游筛掉了"那一族** ✓（与已修的 6 条同族 ✓）。
//
// **∴ 判什么（三半都要 ✓ —— 少了任何一半，别的错误都会冒充通过 ✗）**：
//   ① 纯点列多边形（**故意不给 `bbox`** ✗）建得出来 ✓；
//   ② **选区外零像素变化** ✓ —— 用**三角形**当选区 ✓，取"**在其 bbox 内、但在三角形外**"那一点 ✓
//      （若覆盖度其实只按 bbox 算 ✗ ⇒ 这一点会变色 ⇒ **当场抓住** ✓★）；
//   ③ **选区内的点确实落墨** ✓（否则"什么都没画"或"裁得过狠"也会过 ✗）。
//
// 用法：node scripts/tool-selection-clip-polygon.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-selection-clip-polygon.mjs <base-url>");
  process.exit(2);
}
const DOC = "clip_poly_" + Date.now().toString(36);
const call = async (token, tool, args) =>
  (await fetch(base + "/api/tools/" + tool + "?doc=" + DOC + "&token=" + token, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const px = async (token, x, y) => {
  const reply = await call(token, "render_region", { region: { x: x, y: y, w: 1, h: 1 }, raw: true });
  const url = reply && (reply.raw_url || reply.rawUrl);
  if (!url) return null;
  const absolute = String(url).startsWith("http") ? String(url) : base + url;
  return Buffer.from(await (await fetch(absolute)).arrayBuffer());
};

const created = await (await fetch(base + "/api/documents", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: 200, height: 150 }),
})).json();
if (!created || !created.token) {
  console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
  process.exit(3);
}
const token = created.token;
await call(token, "create_layer", { layer_id: "L1", name: "L1" });

// **三角形** ✓（非矩形 ✓）：bbox 是 (20,20)–(180,130)，而 (30,120) 落在 bbox 内、**三角形外** ✓
const triangle = [[20, 20], [180, 20], [100, 130]];
const selection = await call(token, "create_selection", {
  selection_id: "poly", shape: { kind: "polygon", points: triangle },
});
console.log("  ① create_selection(polygon，**不给 bbox**) ⇒ ok=" + (selection && selection.ok));
if (!selection || selection.ok !== true) {
  console.error("❌ 判不了：多边形选区建不出来（" + JSON.stringify(selection).slice(0, 140) + "）⇒ 退出码 3");
  process.exit(3);
}

// 三个探针点：① 三角形内 ✓ ② bbox 内、三角形外 ✗ ③ 完全在外 ✗
const probes = [[100, 40], [30, 120], [5, 140]];
const before = [];
for (const p of probes) before.push(await px(token, p[0], p[1]));
if (before.some((bytes) => !bytes)) {
  console.error("❌ 判不了：拿不到基线像素 ⇒ 退出码 3");
  process.exit(3);
}
const stroke = await call(token, "brush_stroke", {
  layer_id: "L1", object_id: "s1", brush: "100%_Opaque", size: 24,
  color: { r: 220, g: 40, b: 40, a: 255 }, points: [[10, 60], [190, 60]], clip_to_selection: "poly",
});
console.log("  ① brush_stroke(clip_to_selection=poly) ⇒ ok=" + (stroke && stroke.ok) + "｜" + JSON.stringify(stroke && stroke.context || {}).slice(0, 110));
if (!stroke || stroke.ok !== true) {
  console.error("❌ 判不了：笔触本身失败 ⇒ 退出码 3（**不许**拿「没落墨」当产品缺陷 ✗）");
  process.exit(3);
}
const after = [];
for (const p of probes) after.push(await px(token, p[0], p[1]));
const changed = probes.map((p, i) => !before[i].equals(after[i]));
console.log("  ② 三角形内 (100,40) 变了=" + changed[0] + "（需 true）");
console.log("  ② bbox 内、三角形外 (30,120) 变了=" + changed[1] + "（需 false）");
console.log("  ③ 完全在外 (5,140) 变了=" + changed[2] + "（需 false）");

const failures = [];
if (!changed[0]) failures.push("**三角形内没落墨** ✗ ⇒ 判据无效（不能证明任何事 ✓）");
if (changed[1]) failures.push("**三角形外（bbox 内）落了墨** ✗ ⇒ 裁剪其实只按 bbox 算 ⇒ 只覆盖矩形 ✓");
if (changed[2]) failures.push("完全在外的点也变了 ✗");
if (failures.length) {
  console.error("❌ " + failures.length + " 项没成立：");
  for (const item of failures) console.error("   · " + item);
  process.exit(1);
}
console.log("✓ 纯点列多边形可裁剪 ✓：内落墨 ✓、三角形外零变化 ✓");
