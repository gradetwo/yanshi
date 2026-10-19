#!/usr/bin/env node
// **`brush_stroke(clip_to_selection=…)` 必须能用 `rect` 选区裁剪**（第 901 轮；真 bug）
//
//  根因：`create_selection` 存的 shape 是 `{kind:"rect", x, y, w, h}`
//        而 `tools.rs` 的 clip 解析读的是 `num("width")` / `num("height")`
//        ⇒ **永远 None** ⇒ 被 `filter_map` 丢掉 ⇒ 报「找不到可裁剪的选区 ⇒ 现有：（一个都没有）」
//  ⇒ 这条路径**从来没连通**；判据锁住「存什么字段就读什么字段」。
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-clip-to-selection-rect.mjs <base-url>"); process.exit(2); }
const doc = `clip-${process.pid}-${Date.now() % 100000}`;
trackTemp(mkdtempSync(join(tmpdir(), "clip-rect-")));
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 100, height: 100 }),
})).json();
if (!created.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
// 每个形状用**独立文档**：否则前一段的墨会污染后一段的采样点
// （第 916 轮实测：(45,45) 落在 rect 段选区 10..60 内 ⇒ 读到的是那段的红墨）
const mkCall = (docId, token) => (tool, args) =>
  fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  }).then((r) => r.json());
let docSeq = 0;
const freshCall = async () => {
  const id = `${doc}-${docSeq++}`;
  const c = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 100, height: 100 }),
  })).json();
  if (!c.token) { console.error("  ⊘ 不可判：建文档失败（判据前提不成立）"); process.exit(3); }
  return mkCall(id, c.token);
};
const call = mkCall(doc, created.token);

const failures = [];

// **四种形状都要能被裁剪**（第 905 轮扩：原来只测 rect）
//   每个用「内点 / 外点」两个采样；内点必须红、外点必须白。
const cases = [
  // [名字, 形状, 内点, 外点]
  ["rect", { kind: "rect", x: 10, y: 10, w: 50, h: 50 }, [30, 30], [80, 80]],
  ["ellipse", { kind: "ellipse", cx: 50, cy: 50, rx: 30, ry: 30 }, [50, 50], [5, 5]],
  ["polygon", { kind: "polygon", points: [[10, 10], [80, 10], [80, 80], [10, 80]] }, [40, 40], [95, 95]],
];
for (const [name, shape, insidePt, outsidePt] of cases) {
  const id = `s-${name}`;
  const layer = `l-${name}`;
  const ownCall = await freshCall();
  const sel = await ownCall("create_selection", { selection_id: id, shape });
  if (sel.ok !== true) {
    console.error(`  ⊘ 不可判：建不了 ${name} 选区（判据前提不成立）`);
    process.exit(3);
  }
  await ownCall("create_layer", { layer_id: layer, name: layer });
  const r = await ownCall("brush_stroke", {
    layer_id: layer, brush: "100%_Opaque.myb", size: 30,
    points: [[15, 15], [85, 85]], clip_to_selection: id, color: [1, 0, 0, 1],
  });
  if (r.ok !== true) {
    failures.push(`${name}: brush_stroke(clip=${id}) 被拒 ⇒ ${JSON.stringify(r.context ?? {}).slice(0, 100)}`);
    continue;
  }
  const px = await ownCall("sample_colors", { points: [insidePt, outsidePt] });
  const [ins, out] = (px.samples ?? []).map((s) => (s.rgba ?? []).slice(0, 3).join(","));
  console.log(`  ${name.padEnd(8)} ok=true｜内${JSON.stringify(insidePt)}=${ins}｜外${JSON.stringify(outsidePt)}=${out}`);
  if (ins !== "255,0,0") failures.push(`${name}: 内点没落墨（${ins}）⇒ clip 没生效`);
  if (out === "255,0,0") failures.push(`${name}: 外点也落墨（${out}）⇒ clip 没裁住`);
}

// **bitmap**：用 subtract 产生（矩形 − 内部小矩形），洞与外都要被排除
{
  const bmCall = await freshCall();
  const selA = await bmCall("create_selection", { selection_id: "s-bm", shape: { kind: "rect", x: 10, y: 10, w: 70, h: 70 } });
  const selB = await bmCall("create_selection", { selection_id: "s-bm", mode: "subtract", shape: { kind: "rect", x: 35, y: 35, w: 20, h: 20 } });
  if (selA.ok !== true || selB.ok !== true) {
    console.error("  ⊘ 不可判：建不了 bitmap 选区（判据前提不成立）");
    process.exit(3);
  }
  const listed = await bmCall("list_selections", {});
  const bm = (listed.selections ?? []).find((x) => x.selection_id === "s-bm");
  if (!bm || bm.shape?.kind !== "bitmap") {
    console.error(`  ⊘ 不可判：subtract 之后不是 bitmap（kind=${bm?.shape?.kind}）`);
    process.exit(3);
  }
  await bmCall("create_layer", { layer_id: "l-bm", name: "l-bm" });
  const r = await bmCall("brush_stroke", {
    layer_id: "l-bm", brush: "100%_Opaque.myb", size: 30,
    points: [[15, 15], [75, 75]], clip_to_selection: "s-bm", color: [1, 0, 0, 1],
  });
  if (r.ok !== true) {
    failures.push(`bitmap: brush_stroke 被拒 ⇒ ${JSON.stringify(r.context ?? {}).slice(0, 100)}`);
  } else {
    const px = await bmCall("sample_colors", { points: [[20, 20], [45, 45], [95, 95]] });
    const [body, hole, out] = (px.samples ?? []).map((s) => (s.rgba ?? []).slice(0, 3).join(","));
    console.log(`  bitmap   ok=true｜裙身(20,20)=${body}｜洞里(45,45)=${hole}｜真外侧(95,95)=${out}`);
    if (body !== "255,0,0") failures.push(`bitmap: 裙身没落墨（${body}）`);
    if (hole === "255,0,0") failures.push(`bitmap: 洞里被涂（${hole}）⇒ 减法没进裁剪`);
    if (out === "255,0,0") failures.push(`bitmap: 真外侧被涂（${out}）⇒ bbox 没裁住`);
  }
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 四种形状（rect/ellipse/polygon/bitmap）的 clip_to_selection 全部正确");
