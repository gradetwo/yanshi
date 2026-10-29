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
  // **★ 第 1164 轮修**：建文档 API 认 **`doc_id`**，**不认 `doc`** ✗
  //   ⇒ 原来写 `doc:` ⇒ API 忽略它 ⇒ 返回**已有的** `_b`（runner 起的 64×64）
  //   ⇒ 于是判据描述的是 `_b` ⇒ 报「width = 64（期望 200）」⇒ **假红** ✗
  //   实测：`{"doc_id":"dc_x","width":200,"height":150}` ⇒ doc_id=dc_x｜w=200｜h=150 ✓
  body: JSON.stringify({ doc_id: `describe_canvas_${Date.now()}`, width: W, height: H }),
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

// ★ **`estimated_memory_bytes` 必须是内存量级** ★**（报告 5/6 第 1 项 ✓）
//   ⇒ **∴ 旧口径** `存活对象 × 180` 是序列化量级，实测仅为 RSS 的约 1/882 ✗
//   ⇒ **∴ 现口径**：原子数 ≥ 100 时 `estimated = bytes_per_atom_avg × 原子数`
//     （≈ 进程 RSS）；< 100 时 avg 报 0 = 不可判，`estimated` 退为进程 RSS 字节数。
//   ⇒ **∴ 小文档不断言跨次增长**：RSS 以整 MB 计，20 笔未必跨 MB 边界，
//     跨次比较会抖；**断言同回执恒等式**（不跨次、不跨机器）：
//     `estimated == bytes_per_atom_avg × atoms`，且
//     `0 ≤ server_rss_mb × MiB − estimated < atoms`（floor 取整的余数性质）。
//   ⇒ **∴ 旧的“每笔 100..100000 字节”区间已删**：那是序列化量级的残留；
//     内存/原子跨画布/笔刷实测在约 95 KB..300 KB 间摆动，绝对每笔区间不是契约。
const memBefore = canvas.estimated_memory_bytes;
const objectsBefore = (canvas.layers ?? []).reduce((n, l) => n + (l.objects ?? 0), 0);
console.log(`  ${Number.isFinite(memBefore) ? "✓" : "✗"} estimated_memory_bytes = ${memBefore}（画之前，objects=${objectsBefore}）`);
if (!Number.isFinite(memBefore)) bad += 1;
const memoryScale = Number.isFinite(memBefore) && memBefore >= 1_000_000;
console.log(`  ${memoryScale ? "✓" : "✗"} estimated_memory_bytes 必须上 MB 量级（旧 180 口径只给数百～数万字节）`);
if (!memoryScale) bad += 1;

const strokeCount = 20;
const paint = async (i) => {
  await call("brush_stroke", {
    layer_id: "dc_a", brush: "100%_Opaque",
    color: { r: 90, g: 70, b: 50, a: 255 },
    points: [[20, 20 + (i * 7) % 110, 1], [180, 20 + (i * 7) % 110, 1]],
    size: 8, preview: false,
  });
};
for (let i = 0; i < strokeCount; i += 1) await paint(i);
const after = await call("describe_canvas", {});
const memAfter = after.estimated_memory_bytes;
const objectsAfter = (after.layers ?? []).reduce((n, l) => n + (l.objects ?? 0), 0);
const objectsGrew = objectsAfter > objectsBefore;
console.log(`  ${objectsGrew ? "✓" : "✗"} 画 ${strokeCount} 笔后 ⇒ objects ${objectsBefore}→${objectsAfter}｜字节 ${memBefore}→${memAfter}（对象必须增长；字节跨次不比，见上）`);
if (!objectsGrew) bad += 1;
// **大档恒等式**：再画到原子数 ≥ 100（avg 生效的门槛），同一回执内验恒等式。
let large = after;
let extra = 0;
while ((large.counts?.atoms ?? 0) < 100 && extra < 160) { await paint(strokeCount + extra); extra += 1; large = await call("describe_canvas", {}); }
const atomsLarge = large.counts?.atoms;
const avgLarge = large.bytes_per_atom_avg;
const estLarge = large.estimated_memory_bytes;
const rssMbLarge = large.server_rss_mb;
console.log(`  大档：补画 ${extra} 笔后 atoms=${atomsLarge}｜avg=${avgLarge}｜estimated=${estLarge}｜server_rss_mb=${rssMbLarge}`);
const identity = Number.isFinite(atomsLarge) && atomsLarge >= 100 && Number.isFinite(avgLarge) && avgLarge > 0
  && estLarge === avgLarge * atomsLarge;
console.log(`  ${identity ? "✓" : "✗"} 大档恒等式 estimated == bytes_per_atom_avg × atoms`);
if (!identity) bad += 1;
if (Number.isFinite(rssMbLarge)) {
  const rssBytes = rssMbLarge * 1024 * 1024;
  const gap = rssBytes - estLarge;
  const bound = Number.isFinite(gap) && gap >= 0 && gap < atomsLarge;
  console.log(`  ${bound ? "✓" : "✗"} 大档截断界 0 ≤ server_rss_mb×MiB − estimated < atoms（实测 gap=${gap}）`);
  if (!bound) bad += 1;
} else {
  console.log("  ✗ 大档 server_rss_mb 非数字（Linux 上应可读 /proc）");
  bad += 1;
}

const selOk = (canvas.selections ?? []).some((s) => s.selection_id === "dc_s");
console.log(`  ${selOk ? "✓" : "✗"} selections 里能找到刚建的 dc_s`);
if (!selOk) bad += 1;

if (bad) { console.log(`  ✗ ${bad} 处不符（describe_canvas 的契约）`); process.exit(1); }
console.log("  ✓ describe_canvas 一次给全：尺寸／层／选区／掩码，且层数与 list_layers 一致");
