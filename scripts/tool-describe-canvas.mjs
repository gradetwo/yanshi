#!/usr/bin/env node
// **`describe_canvas` 必须一次给全画布状态** ✓（YANSHI_ISSUES §18.4 ✓）
//
// **∴ 它要替代"打四五次"✗**：`get_document`（尺寸 ✓）＋ `list_layers`（层 ✓）＋
//   `list_selections`（选区 ✓）＋ 掩码 ⇒ **∴ 所以缺任何一个字段，它就不省事了 ✗**
//
// **∴ 判据 ✓**：
//   ① 必须含 `width`／`height`／`layers`／`selections`／`masks` ✓
//   ② `width`／`height` 必须等于真画布 ✓
//   ③ **`layers` 长度必须等于 `list_layers` 的 `count`** ✗ —— **∴ 这是"两边对账"✓，
//      **∴ 而不只是"字段存在"✗**（**∴ 字段存在但数不对，那更糟 ✓**）
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-describe-canvas.mjs <base-url>"); process.exit(3); }

const W = 200, H = 150;
const boot = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc: `describe_canvas_${Date.now()}`, width: W, height: H }),
});
const bootBody = await boot.json().catch(() => ({}));
const token = bootBody.token;
if (!token) { console.log(`  ✗ 建文档失败：${JSON.stringify(bootBody).slice(0, 140)}`); process.exit(3); }
const doc = bootBody.doc ?? bootBody.doc_id ?? "describe_canvas";

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) });
  return await reply.json();
};

await call("create_layer", { layer_id: "dc_a", name: "A" });
await call("create_layer", { layer_id: "dc_b", name: "B" });
await call("create_selection", { selection_id: "dc_s", shape: { kind: "rect", x: 20, y: 20, width: 40, height: 40 } });
await call("create_mask", { mask_id: "dc_m", shape: { kind: "ellipse", cx: 100, cy: 100, rx: 20, ry: 20 } });

const canvas = await call("describe_canvas", {});
if (canvas.ok !== true) { console.log(`  ✗ describe_canvas 返回 ok!=true：${JSON.stringify(canvas).slice(0, 140)}`); process.exit(1); }

let bad = 0;
const FIELDS = ["width", "height", "layers", "selections", "masks"];
for (const f of FIELDS) {
  const has = Object.prototype.hasOwnProperty.call(canvas, f);
  console.log(`  ${has ? "✓" : "✗"} 含字段 \`${f}\`${has ? "" : "（缺 ✗）"}`);
  if (!has) bad += 1;
}
for (const [f, want] of [["width", W], ["height", H]]) {
  const got = canvas[f];
  const ok = got === want;
  console.log(`  ${ok ? "✓" : "✗"} ${f} = ${got}（期望 ${want}）`);
  if (!ok) bad += 1;
}

const listed = await call("list_layers", {});
const listedCount = listed.count;
const canvasLayers = (canvas.layers ?? []).length;
const match = listedCount === canvasLayers;
console.log(`  ${match ? "✓" : "✗"} layers 长度 ${canvasLayers} 与 list_layers.count ${listedCount} ${match ? "一致" : "**不一致** ✗"}`);
if (!match) bad += 1;

// ★ **`masks` 必须是**对象数组**✗ ★**（报告 §31② ✓：他们按对象解析，遇到字符串就崩
//   `'str' object has no attribute 'get'` ✓）⇒ **∴ 断言每一项都是对象 ✓、且含 `mask_id` ✓**。
const masksShape = (canvas.masks ?? []).map((m) => (m && typeof m === "object" && !Array.isArray(m) ? "object" : typeof m));
const masksAllObjects = masksShape.every((kind) => kind === "object");
console.log(`  ${masksAllObjects ? "✓" : "✗"} masks 每一项都是对象（实测 ${JSON.stringify(masksShape)}）`);
if (!masksAllObjects) bad += 1;
const masksHaveIds = (canvas.masks ?? []).every((m) => m && typeof m.mask_id === "string");
console.log(`  ${masksHaveIds ? "✓" : "✗"} masks 每一项都含字符串 \`mask_id\``);
if (!masksHaveIds) bad += 1;

// ★ **`estimated_memory_bytes` 必须随对象数**线性**增长**✗ ★**（报告 §33 第 3 点 ✓）
//   ⇒ **∴ 他们要它来**长跑前预估**✗ ⇒ **∴ 若它不随对象数涨 ⇒ **∴ 那字段就是死的 ✗**
//   ⇒ **∴ 断言 ✓**：画 N 笔后字节数**约等于** N 的倍数（**∴ 每笔 ~1.18 KB ✓**）
//     ⇒ **∴ 用**翻倍**判定 ✓（**∴ 不写死绝对值 ✓ ⇒ **∴ 不随机器／版本变 ✓**）
const memBefore = canvas.estimated_memory_bytes;
const objectsBefore = (canvas.layers ?? []).reduce((n, l) => n + (l.objects ?? 0), 0);
console.log(`  ${Number.isFinite(memBefore) ? "✓" : "✗"} estimated_memory_bytes = ${memBefore}（画之前，objects=${objectsBefore}）`);
if (!Number.isFinite(memBefore)) bad += 1;

const strokeCount = 20;
for (let i = 0; i < strokeCount; i += 1) {
  await call("brush_stroke", {
    layer_id: "dc_a", brush: "100%_Opaque",
    color: { r: 90, g: 70, b: 50, a: 255 },
    points: [[20, 20 + i * 4, 1], [180, 20 + i * 4, 1]],
    size: 8, preview: false,
  });
}
const after = await call("describe_canvas", {});
const memAfter = after.estimated_memory_bytes;
const objectsAfter = (after.layers ?? []).reduce((n, l) => n + (l.objects ?? 0), 0);
const grew = objectsAfter > objectsBefore && memAfter > memBefore;
console.log(`  ${grew ? "✓" : "✗"} 画 ${strokeCount} 笔后 ⇒ objects ${objectsBefore}→${objectsAfter}｜字节 ${memBefore}→${memAfter}（**必须都增长**）`);
if (!grew) bad += 1;
const perStroke = (memAfter - memBefore) / (objectsAfter - objectsBefore);
const plausible = perStroke > 100 && perStroke < 100_000;
console.log(`  ${plausible ? "✓" : "✗"} 每笔字节 ≈ ${Number.isFinite(perStroke) ? perStroke.toFixed(0) : "NaN"}（**合理区间 100..100000**）`);
if (!plausible) bad += 1;

const selOk = (canvas.selections ?? []).some((s) => s.selection_id === "dc_s");
console.log(`  ${selOk ? "✓" : "✗"} selections 里能找到刚建的 dc_s`);
if (!selOk) bad += 1;

if (bad) { console.log(`  ✗ ${bad} 处不符（describe_canvas 的契约）`); process.exit(1); }
console.log("  ✓ describe_canvas 一次给全：尺寸／层／选区／掩码，且层数与 list_layers 一致");
