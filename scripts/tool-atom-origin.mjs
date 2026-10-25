#!/usr/bin/env node
// **原子的来源必须可机械复核**：`kind` 是**存储形态**，`origin` 才是**来源** ✓（第 642 轮；外部 D5）
//
// **⇒ 他们的实测**：一幅 8K 画的 **39,354 条笔触原子 `kind` 全是 `"import_image"`** ✗
//   ⇒ 用它做"严禁导入参考图"的证明，会把"画出来的 38k 笔"误判成"导入了一张图" ✓。
//
// **⇒ 判据**：
//   ① 画一笔（`brush_stroke`）⇒ 那条原子的 `origin` 必须是 **`drawn`** ✓（**不管 `kind` 是什么** ✓）；
//   ② 建图层那类记账原子 ⇒ `origin` 必须是 **`system`** ✓；
//   ③ 每条原子都必须带 `origin` 字段（不许缺 ⇒ 缺失会让调用方又退回去看 `kind` ✗）。
//   ④ `import_image` 导入一张图 ⇒ 对应原子的 `origin` 与 `stored_origin` 都必须是 **`imported`** ✓
//      （`put_blob` 先造 blob，再 `import_image`；导入臂与 ① 的 drawn 臂互为对照）。
//
// **变异**：把 `origin` 恒写成 `"imported"`（或删掉该字段）⇒ ① / ③ 红 ✓；
//   把 import 的 `origin` 写成 `"drawn"` ⇒ ④ 红 ✓。
import { writeFileSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import zlib from "node:zlib";
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-atom-origin.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};
const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
const summary = await call("get_document", {});
const docW = Number(summary.width ?? 64);
const docH = Number(summary.height ?? 64);
const stroke = await call("brush_stroke", {
  layer_id: "layer_default",
  brush: "calligraphy",
  points: [
    [Math.round(docW * 0.2), Math.round(docH * 0.3), 0.5],
    [Math.round(docW * 0.8), Math.round(docH * 0.7), 0.6],
  ],
  size: Math.max(6, Math.round(Math.min(docW, docH) * 0.1)),
  color: [0.2, 0.4, 0.9, 1.0],
});
if (stroke.ok !== true) fail(`brush_stroke ⇒ ${JSON.stringify(stroke).slice(0, 160)}`);

const listed = await call("list_atoms", { limit: 50, offset: 0, include_non_stroke: true });
if (listed.ok !== true) fail(`list_atoms ⇒ ${JSON.stringify(listed).slice(0, 160)}`);
const atoms = listed.atoms ?? [];
if (atoms.length === 0) {
  console.log("  ⊘ 不可判：一条原子都没列出来");
  process.exit(3);
}
const missing = atoms.filter((atom) => typeof atom.origin !== "string");
if (missing.length > 0) {
  fail(`有 ${missing.length} 条原子缺 origin ⇒ 调用方只能退回去看 kind（这正是 D5 的病）`);
}
const drawn = atoms.filter((atom) => atom.origin === "drawn");
if (drawn.length === 0) {
  fail(`刚画的那一笔没有 origin=drawn ⇒ 来源不可机械复核（清单：${atoms.map((a) => `${a.kind}/${a.origin}`).slice(0, 6).join(", ")}）`);
}
// **新原子必须把来源写进原子本身** ✓（第 659 轮；外部 D5 的后半句）
//   ⇒ `stored_origin` 是**落账时写死的**值；老日志为 null（只有派生兜底 ✓）
const storedMissing = drawn.filter((atom) => atom.stored_origin !== "drawn");
if (storedMissing.length > 0) {
  fail(
    `刚画的笔没有把来源写进原子（stored_origin=${JSON.stringify(storedMissing[0].stored_origin)}）⇒ 只有"读时派生"，不是写入方声明`,
  );
}
const system = atoms.filter((atom) => atom.origin === "system");
if (system.length === 0) {
  fail(`建图层那类记账原子没有 origin=system ⇒ 来源分类不成立`);
}
// **④ 导入臂**：`import_image` 导入一张图 ⇒ 对应原子 `origin` 与 `stored_origin` 都必须是 `imported`
//   ⇒ 构造法与 `tool-import-blob-decodable.mjs` 同款：先写一张 8×8 纯红 PNG 到 root，
//     `put_blob` 造 blob，再 `import_image`。
const exportDir = process.env.YANSHI_EXPORT_DIR;
if (!exportDir) {
  console.log("  ⊘ 不可判：没设 YANSHI_EXPORT_DIR（导入臂要靠它反推 root）");
  process.exit(3);
}
const root = dirname(exportDir);
const crcTable = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};
const W = 8;
const H = 8;
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8;
ihdr[9] = 6;
const rawRows = Buffer.alloc(H * (1 + W * 4));
for (let y = 0; y < H; y += 1) {
  for (let x = 0; x < W; x += 1) {
    const off = y * (1 + W * 4) + 1 + x * 4;
    rawRows[off] = 200;
    rawRows[off + 1] = 40;
    rawRows[off + 2] = 40;
    rawRows[off + 3] = 255;
  }
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(rawRows)),
  chunk("IEND", Buffer.alloc(0)),
]);
const pngPath = join(root, `probe_import_${process.pid}.png`);
try {
  writeFileSync(pngPath, png);
} catch (e) {
  console.log(`  ⊘ 不可判：写不进 root（${e.message}）`);
  process.exit(3);
}
const put = await call("put_blob", { path: pngPath });
if (put.ok !== true || !put.blob_hash) {
  console.log(`  ⊘ 不可判：put_blob 失败 ⇒ 导入臂的前提不成立（${JSON.stringify(put).slice(0, 120)}）`);
  process.exit(3);
}
const imported = await call("import_image", {
  layer_id: "layer_default",
  bitmap: { blob_hash: put.blob_hash, size: 0, mime_type: "image/png" },
  region: { x: 10, y: 10, w: W, h: H },
});
if (imported.ok !== true) fail(`④ import_image ⇒ ${JSON.stringify(imported).slice(0, 160)}`);
try {
  rmSync(pngPath);
} catch {
  /* 清理失败不影响判据 */
}
const listedAfter = await call("list_atoms", { limit: 50, offset: 0, include_non_stroke: true });
if (listedAfter.ok !== true) fail(`④ list_atoms ⇒ ${JSON.stringify(listedAfter).slice(0, 160)}`);
const importedAtoms = (listedAfter.atoms ?? []).filter(
  (atom) => atom.origin === "imported" && atom.stored_origin === "imported",
);
if (importedAtoms.length === 0) {
  fail(
    `④ import_image 导入的图没有 origin=imported / stored_origin=imported 的原子 ⇒ 来源不可机械复核（清单：${(listedAfter.atoms ?? []).map((a) => `${a.kind}/${a.origin}/${a.stored_origin}`).slice(0, 6).join(", ")}）`,
  );
}
const importedWithSource = drawn.filter((atom) => atom.kind === "import_image");
console.log(
  `✓ origin 可复核：${drawn.length} 条 drawn（其中 ${importedWithSource.length} 条 kind 仍是 import_image ⇒ 正是 D5 说的"存储形态≠来源"）、${system.length} 条 system、${importedAtoms.length} 条 imported、共 ${(listedAfter.atoms ?? []).length} 条`,
);
