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
const brushes = raw.map((item) => (typeof item === "string" ? item : item.name || item.id || "")).filter(Boolean);
if (!brushes.length) {
  console.error("没取到笔刷清单 ⇒ 无法生成；顶层键：" + JSON.stringify(Object.keys(list)));
  process.exit(1);
}
console.log(`  笔刷清单：${brushes.length} 支`);

const sanitize = (name) => name.replace(/[^A-Za-z0-9._-]/g, "_");
const index = { generated_from: "brush_preview", size: 24, files: {}, skipped: {} };
const used = new Set();
let made = 0, skipped = 0;
for (const brush of brushes) {
  const preview = await call("brush_preview", { brush, size: 24 });
  if (!preview.ok) {
    index.skipped[brush] = (preview.context || {}).detail || "被拒绝";
    skipped += 1;
    continue;
  }
  const png = Buffer.from(await fetch(`${base}/api/blob/${preview.blob_hash}?doc=${doc}&token=${token}`).then((r) => r.arrayBuffer()));
  let file = sanitize(brush) + ".png";
  let n = 2;
  while (used.has(file)) { file = sanitize(brush) + "_" + n + ".png"; n += 1; }
  used.add(file);
  writeFileSync(`${outDir}/${file}`, png);
  index.files[brush] = file;
  made += 1;
}
writeFileSync(`${outDir}/index.json`, JSON.stringify(index, null, 2) + "\n");
console.log(`  生成 ${made} 张预览 ✓；服务端拒绝 ${skipped} 支（记进 index.json 的 skipped ✓）`);
console.log(`  输出目录：${outDir}`);
