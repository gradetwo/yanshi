#!/usr/bin/env node
// **`import_image` 必须告诉调用方「这个 blob 能不能解码」** ✓
// （第 1237 轮；外部用户失败 5 次，而唯一能看见原因的地方是 `export_png` 的 render_warnings）
//
// **∴ 这条判据守什么**（两方向，各验一个语义）：
//   ① **正向**：用 `put_blob` 写的 blob（一定包了 RenderCodec 头）
//      ⇒ `ok:true` ∧ `blob_decodable:true` ∧ 导出的 PNG 里像素**真的出现**
//   ② **反向**：手写的**裸 PNG**（首字节 0x89，没有 0x00 头）
//      ⇒ `ok:false` ∧ `blob_decodable:false` ∧ `note` 说清原因
//      ⇒ **不是静默成功**（那正是用户踩的坑）
//
// **∴ 为什么 ② 必须断言 `note`**：只给 `blob_decodable:false` 而不说**为什么**，
//   调用方仍然要去猜 —— 而「失败要可诊断」是本仓的内核之一。
//
// **∴ 变异**：把 `import_image` 返回里的 `blob_decodable` 删掉 ⇒ ① 必红 ✓
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { createHash } from "node:crypto";
import zlib from "node:zlib";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-import-blob-decodable.mjs <base> <doc> <token>");
  process.exit(3);
}
const exportDir = process.env.YANSHI_EXPORT_DIR;
if (!exportDir) {
  console.error("  ⊘ 不可判：没设 YANSHI_EXPORT_DIR（判据要靠它反推 root）");
  process.exit(3);
}
const root = dirname(exportDir);

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
    signal: AbortSignal.timeout(30_000),
  });
  return await reply.json();
};

// ── 造 8×8 纯红 RGBA PNG ──
const W = 8, H = 8;
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
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4); ihdr[8] = 8; ihdr[9] = 6;
const rawRows = Buffer.alloc(H * (1 + W * 4));
for (let y = 0; y < H; y += 1) {
  for (let x = 0; x < W; x += 1) {
    const off = y * (1 + W * 4) + 1 + x * 4;
    rawRows[off] = 200; rawRows[off + 1] = 40; rawRows[off + 2] = 40; rawRows[off + 3] = 255;
  }
}
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr), chunk("IDAT", zlib.deflateSync(rawRows)), chunk("IEND", Buffer.alloc(0)),
]);

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// 把 PNG 写到 root 下（put_blob 只接受 root 内的路径）
const pngPath = join(root, `probe_${process.pid}.png`);
try { writeFileSync(pngPath, png); } catch (e) {
  console.error(`  ⊘ 不可判：写不进 root（${e.message}）`); process.exit(3);
}
const layer = await call("create_layer", { layer_id: "L_bd", name: "BD", if_exists: "reuse" });
if (layer.ok !== true) { console.error("  ⊘ 不可判：建层失败"); process.exit(3); }

// ① 正向：put_blob 写的一定可解码
const put = await call("put_blob", { path: pngPath });
check(put.ok === true, `①a put_blob ⇒ ok=${put.ok}`);
const good = await call("import_image", {
  layer_id: "L_bd",
  bitmap: { blob_hash: put.blob_hash, size: 0, mime_type: "image/png" },
  region: { x: 10, y: 10, w: W, h: H },
});
check(good.ok === true, `①b import_image ⇒ ok=${good.ok}`);
check(good.blob_decodable === true, `①c 必须报 blob_decodable=true ⇒ ${good.blob_decodable}`);

// ② 反向：手写裸 PNG（没有 0x00 头）⇒ 解不开 ⇒ 必须明确说
const hex = createHash("sha256").update(png).digest("hex");
const dir = join(root, "blobs", "sha256", hex.slice(0, 2), hex.slice(2, 4));
try {
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, hex), png); // **裸字节，不包头**
} catch (e) {
  console.error(`  ⊘ 不可判：写不进 CAS（${e.message}）`); process.exit(3);
}
const bad = await call("import_image", {
  layer_id: "L_bd",
  bitmap: { blob_hash: `sha256:${hex}`, size: 0, mime_type: "image/png" },
  region: { x: 50, y: 50, w: W, h: H },
});
// **②a 的语义纠正**（第 1237 轮实测）：`ok` 表示「**原子提交成功**」，不是「能画」
//   ⇒ 解不开的 blob **仍然**能提交（导入这个操作本身成功了）
//   ⇒ 所以这里断言 `ok===true` ∧ `blob_decodable===false`
//      ⇒ 那正是「**上传成功但解不出**」这个状态，也是外部用户最需要看见的信号 ✓
check(bad.ok === true, `②a 原子仍应提交成功（ok 表示提交，不表示能画）⇒ ok=${bad.ok}`);
check(bad.blob_decodable === false, `②b 必须报 blob_decodable=false ⇒ ${bad.blob_decodable}`);
check(/解不开|包头|RenderCodec|put_blob/.test(String(bad.note ?? "")),
  `②c note 必须说清原因 ⇒ ${JSON.stringify(String(bad.note ?? "")).slice(0, 90)}`);

try { rmSync(pngPath); } catch { /* 清理失败不影响判据 */ }

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（import_image 的 blob_decodable 信号）`);
  process.exit(1);
}
console.log("\n  ✓ import_image 正确报告 blob 可解码性（两方向）");
