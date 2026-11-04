#!/usr/bin/env node
// 预生成画笔库的预览图并**入库**（用户裁定：提前生成好入 git 库，面板直接用图片）。
//
// 用法：node scripts/generate-brush-previews.mjs <server-base> [--out assets/brush-previews]
//
// 用服务端的 brush_preview（与落笔同一条实现）逐支画一小笔 ⇒ 存成 PNG ⇒ 再写一份 index.json
// 把「笔刷名 → 文件名」映射记下来 —— 因为笔刷名里有 `#`、`%` 这类不能直接进 URL 的字符。
// 服务端拒绝的（会读画布的涂抹类）不生成，记进 index.json 的 skipped，客户端回退到工具调用。
import { mkdirSync, writeFileSync } from "node:fs";

const base = process.argv[2];
const outArg = process.argv.indexOf("--out");
const outDir = outArg > 0 ? process.argv[outArg + 1] : "assets/brush-previews";
if (!base) { console.error("用法: node scripts/generate-brush-previews.mjs <server-base> [--out DIR]"); process.exit(2); }
mkdirSync(outDir, { recursive: true });
const doc = "bp1";
const created = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
}).then((r) => r.json());
const token = created.token;
if (!token) { console.error("建文档没拿到 token ⇒ 无法生成（不是成功）"); process.exit(1); }
const call = (tool, args) =>
  fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  }).then(async (r) => (r.ok ? r.json() : r.json().catch(() => ({ ok: false }))));

// 真实形状（实测）：顶层 `assets`（还有 count/hint/kind）；元素可能是字符串或对象。
const list = await call("list_assets", { kind: "brush" });
const raw = list.assets || (list.data && list.data.assets) || [];
// **只认 .myb** —— 实测清单一度混进过 CASE-NOTES.md 这类文档文件（它不是笔刷）。
const brushes = raw
  .map((item) => (typeof item === "string" ? item : item.name || item.id || ""))
  .filter((name) => name.endsWith(".myb"));
if (!brushes.length) {
  console.error("没取到笔刷清单 ⇒ 无法生成；顶层键：" + JSON.stringify(Object.keys(list)));
  process.exit(1);
}
console.log(`  笔刷清单：${brushes.length} 支`);

const sanitize = (name) => name.replace(/[^A-Za-z0-9._-]/g, "_");

// **被拒绝的笔刷也要有能看懂的图**（用户裁定）：
// 涂抹类在空白底子上当然画不出东西 —— 那就**先铺一层色底**，再用它落一笔，取那一笔的区域当预览
// （"在已有颜色上抹一下"正是它的用法）。三级回退，按顺序试，第一个出图的就用它，并记下用了哪一级。
const fetchPng = async (blobHash) =>
  Buffer.from(await fetch(`${base}/api/blob/${blobHash}?doc=${doc}&token=${token}`).then((r) => r.arrayBuffer()));
const fallbacks = [
  { method: "size64", run: () => call("brush_preview", { brush: 0, size: 64 }) },
  { method: "pressure1", run: () => call("brush_preview", { brush: 0, size: 32,
      points: [[16, 16, 1], [40, 22, 1], [64, 16, 1]] }) },
  { method: "on_base", run: null },
];
// **★ 每次跑都要一个**新的图层/对象名** ✗ ★**（2026-11-05 实测 ✓）
//   **∴ 症状 ✗**：`doc` 固定 `bp1` ✓、`baseLayer` 固定 `Lprev` ✓、对象 id 按笔刷名固定 ✓
//     ⇒ **第二次生成**时这些 id **已存在** ⇒ `brush_stroke` 失败 ⇒ **19 支回退笔刷静默消失** ✗
//       （实测：179 张 vs 应有 196；`ramon-Knife`／`smudge`／`oil-*` 全被跳过 ✓）
//     ⇒ 而日志只说「仍无图 21 支」✗ —— 看起来像笔刷本身不行 ✓。
//   **∴ 修法** ✓：每次用一个新标签 ⇒ id 不撞 ✓；doc 仍复用（不留一堆文档 ✓）。
const runTag = Date.now().toString(36);
const baseLayer = "Lprev_" + runTag;
await call("create_layer", { layer_id: baseLayer, name: "preview base" });
// **★ 底色要**双色硬边**✗ 单色看不出一支涂抹笔刷在干什么 ★**（2026-11-05 ✓）：
//   **∴ 实测 ✗**：单色蓝带 ⇒ 回退出来的预览**就是一整条蓝** ✓（Blender／oil-* 全一样 ✗）
//     ⇒ 笔刷库里三十支笔**长得一模一样** ⇒ 预览失去意义 ✓。
//   **∴ 做法** ✓：蓝｜橙两段在 x=80 处**硬接** ⇒ 笔刷抹过接缝就能看见混色 ✓。
const paintBand = (suffix) => Promise.all([
  call("brush_stroke", { layer_id: baseLayer, object_id: "baseband_a_" + runTag + "_" + suffix, brush: "100%_Opaque", size: 48,
    color: { r: 60, g: 130, b: 200, a: 255 }, points: [[20, 40, 1], [80, 40, 1]] }),
  call("brush_stroke", { layer_id: baseLayer, object_id: "baseband_b_" + runTag + "_" + suffix, brush: "100%_Opaque", size: 48,
    color: { r: 226, g: 138, b: 52, a: 255 }, points: [[80, 40, 1], [150, 40, 1]] }),
]);
await paintBand("initial");
const rawPreview = async (brush, index) => {
  for (const step of fallbacks) {
    if (step.method === "on_base") {
      // **每支笔刷一个唯一的对象 id**（复用同一个 id 从第二支起就会撞"已存在"）
      const oid = "smudge_" + brush.replace(/[^A-Za-z0-9._-]/g, "_") + "_" + runTag;
      // **★ 每支涂抹笔刷**自己重铺一次底色** ✗ ★**（2026-11-05 实测 ✓）
      //   **∴ 症状 ✗**：底色只在循环外铺一次 ⇒ 前面几十支涂抹笔刷**把色带抹没了**
      //     ⇒ 后面的笔刷开始报「这块区域上还没有颜色可抹」⇒ **17 支被静默跳过** ✗
      //       （实测：179 张 vs 应有 196 ✓）
      //   **∴ 修法** ✓：落笔前重铺同一条色带 ✓（同一图层、唯一 object id ⇒ 幂等叠加 ✓）。
      await paintBand(String(index));
      const stroke = await call("brush_stroke", { layer_id: baseLayer, object_id: oid, brush, size: 28,
        points: [[26, 40, 1], [144, 40, 1]] });
      if (!stroke.ok) continue;
      const got = await call("get_object", { object_id: oid });
      // **★ 不能直接拿对象位图当预览 ✗ ★**（2026-11-05 实测 ✓）
      //   **∴ 症状 ✗**：`data.bitmap` 是 **`image/x-yanshi-raw`**（RGBA 裸字节 ✓）
      //     ⇒ 原样写成 `.png` ⇒ 浏览器显示**破图** ✗ —— 本仓 **30/185** 张就是这样坏掉的 ✓
      //     （`file` 说 `data`，开头 64 字节全 0 ✓；大小恰好 `w*h*4` ✓）。
      //   **∴ 修法** ✓：走 `render_region`（服务端编码器 ✓）拿**真 PNG** ✓。
      const bbox = got && Array.isArray(got.bbox) ? got.bbox : null;
      if (bbox && bbox.length === 4) {
        const region = { x: bbox[0], y: bbox[1], w: bbox[2], h: bbox[3] };
        const rendered = await call("render_region", { region });
        if (rendered.ok && rendered.blob_hash) {
          return { png: await fetchPng(rendered.blob_hash), method: step.method };
        }
      }
      continue;
    }
    const preview = await step.run.call(null, brush);
    if (preview.ok && preview.blob_hash) return { png: await fetchPng(preview.blob_hash), method: step.method };
  }
  return null;
};
const index = { generated_from: "brush_preview", size: 24, files: {}, skipped: {} };
const used = new Set();
let made = 0, skipped = 0;
for (const brush of brushes) {
  const preview = await call("brush_preview", { brush, size: 24 });
  let png = null;
  let method = "direct";
  if (preview.ok && preview.blob_hash) {
    png = await fetchPng(preview.blob_hash);
  } else {
    // **服务端拒绝 ⇒ 走回退，出图就算成功**（并记下用了哪一级，便于如实说明）。
    const viaFallback = await rawPreview(brush, made + skipped);
    if (viaFallback) { png = viaFallback.png; method = viaFallback.method; }
  }
  if (!png) {
    index.skipped[brush] = (preview.context || {}).detail || "被拒绝";
    skipped += 1;
    continue;
  }
  // **★ 落盘前验 PNG magic ✗ ★**：坏字节写成 `.png` 是**静默降级** ✓
  //   （页面只显示一个破图图标 ✓ 而生成脚本说「生成 N 张 ✓」✗）。
  //   ⇒ 不是 PNG ⇒ 记进 skipped 并**说清**，绝不写出去 ✓。
  if (!png.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) {
    index.skipped[brush] = "返回的不是 PNG（" + png.length + " 字节；mime 可能是 image/x-yanshi-raw）⇒ 拒绝写入";
    skipped += 1;
    continue;
  }
  let file = sanitize(brush) + ".png";
  let n = 2;
  while (used.has(file)) { file = sanitize(brush) + "_" + n + ".png"; n += 1; }
  used.add(file);
  writeFileSync(`${outDir}/${file}`, png);
  index.files[brush] = file;
  if (method !== "direct") { index.fallback = index.fallback || {}; index.fallback[brush] = method; }
  made += 1;
}
writeFileSync(`${outDir}/index.json`, JSON.stringify(index, null, 2) + "\n");
console.log(`  生成 ${made} 张预览 ✓（其中回退出图的 ${Object.keys(index.fallback || {}).length} 支 ✓）；仍无图 ${skipped} 支`);
if (skipped) {
  const reasons = Object.entries(index.skipped).slice(0, 4)
    .map(([k, v]) => k + " → " + String(v).slice(0, 40));
  console.log("  仍无图的前几支原因：" + reasons.join(" ｜ "));
}
if (Object.keys(index.fallback || {}).length) {
  const by = {};
  for (const v of Object.values(index.fallback)) by[v] = (by[v] || 0) + 1;
  console.log("  回退各级用量：" + JSON.stringify(by));
}
console.log(`  输出目录：${outDir}`);
