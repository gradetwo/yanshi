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

const selOk = (canvas.selections ?? []).some((s) => s.selection_id === "dc_s");
console.log(`  ${selOk ? "✓" : "✗"} selections 里能找到刚建的 dc_s`);
if (!selOk) bad += 1;

if (bad) { console.log(`  ✗ ${bad} 处不符（describe_canvas 的契约）`); process.exit(1); }
console.log("  ✓ describe_canvas 一次给全：尺寸／层／选区／掩码，且层数与 list_layers 一致");
