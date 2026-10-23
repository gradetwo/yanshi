#!/usr/bin/env node
// **`import_image` 导入的位图必须**真的画出来** ✓（第 1224 轮；外部 §176 报的 P0）
//
// **∴ 它守的两个 bug**（都是「静默不生效」）：
//   ① 净荷缺 `region`/`width`/`height` ⇒ `parse_raster_patch` 返回 `Unsupported`
//      ⇒ 对象建成了（`describe_canvas` 能数到 `objects=1`）、`ok:true`、`dirty_bbox` 也对
//      ⇒ **但渲染器跳过它** ⇒ 导出 PNG 里一个像素都没有 ✗
//   ② 手写的 blob 文件必须是 **RenderCodec 格式**（`[0x00][TAG][body]`，TAG=0 原始字节），
//      不是裸 PNG ⇒ 否则 `decode` 返回 `None` ⇒ 「无法按当前存储编码解出」✗
//
// **∴ 为什么这条判据必须端到端**：上面两个 bug 里，**任何一个**都会让
//   `ok:true` 而像素为 0 ⇒ 所以断言不能只看 `ok`，**必须解码导出的 PNG 数像素** ✓
//
// **∴ 它怎么拿到服务端 root**：`run-criteria-local.sh` / `run-criteria.sh` 都设
//   `YANSHI_EXPORT_DIR="$ROOT/exports"` ⇒ 所以 `root = dirname(YANSHI_EXPORT_DIR)` ✓
//
// **∴ 变异**：把 `import_image` 净荷里的 `region` 删掉 ⇒ ②③ 必红 ✓
import { decodePng } from "./lib/png.mjs";
import { readFileSync, writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join, dirname } from "node:path";
import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import zlib from "node:zlib";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-import-image-visible.mjs <base> <doc> <token>");
  process.exit(3);
}
const exportDir = process.env.YANSHI_EXPORT_DIR;
if (!exportDir) {
  console.error("  ⊘ 不可判：没设 YANSHI_EXPORT_DIR（判据要靠它反推服务端 root）");
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

// ── 造一个 8×8 的纯红 RGBA PNG（不引外部库） ──
const W = 8, H = 8, RED = [200, 40, 40, 255];
const rawRows = Buffer.alloc(H * (1 + W * 4));
for (let y = 0; y < H; y += 1) {
  rawRows[y * (1 + W * 4)] = 0; // filter: none
  for (let x = 0; x < W; x += 1) {
    const off = y * (1 + W * 4) + 1 + x * 4;
    rawRows[off] = RED[0]; rawRows[off + 1] = RED[1]; rawRows[off + 2] = RED[2]; rawRows[off + 3] = RED[3];
  }
}
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
ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; ihdr[9] = 6; // 8-bit, RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", zlib.deflateSync(rawRows)),
  chunk("IEND", Buffer.alloc(0)),
]);
// **RenderCodec 格式：[0x00][TAG_PLAIN=0][原始字节]**
const stored = Buffer.concat([Buffer.from([0x00, 0x00]), png]);
const hex = createHash("sha256").update(stored).digest("hex");

const fails = [];
const check = (ok, msg) => { if (!ok) fails.push(msg); console.log(`  ${ok ? "✓" : "✗"} ${msg}`); };

// ── 把 blob 写进服务端 CAS ──
const blobDir = join(root, "blobs", "sha256", hex.slice(0, 2), hex.slice(2, 4));
const blobPath = join(blobDir, hex);
try {
  mkdirSync(blobDir, { recursive: true });
  writeFileSync(blobPath, stored);
  console.log(`  ✓ blob 已写入（${stored.length} 字节 = 2 头 + ${png.length} PNG）`);
} catch (e) {
  console.error(`  ⊘ 不可判：写不进服务端 CAS（${blobPath}）：${e.message}`);
  process.exit(3);
}

const layer = await call("create_layer", { layer_id: "L_imp", name: "IMP", if_exists: "reuse" });
check(layer.ok === true, `① 建层 ⇒ ok=${layer.ok}`);

// ② 导入：region 放在 (50,50)，8×8
const imported = await call("import_image", {
  layer_id: "L_imp",
  bitmap: { blob_hash: `sha256:${hex}`, size: stored.length, mime_type: "image/png" },
  region: { x: 50, y: 50, w: W, h: H },
});
const detail = String(imported.context?.detail ?? "");
check(imported.ok === true, `② import_image ⇒ ok=${imported.ok}｜${JSON.stringify(detail).slice(0, 80)}`);

// ③ 导出后**解码数像素** —— 这是本判据的核心（只看 ok 抓不到这两个 bug）
const outName = `imp_${process.pid}.png`;
const exported = await call("export_png", { path: outName, deterministic: true });
if (exported.ok !== true) {
  check(false, `③ export_png ⇒ ok=${exported.ok}`);
} else {
  const buf = readFileSync(join(exportDir, outName));
  // **共用解码器** ✓（第 662 轮：原来这里内联 40 行 ⇒ 与判定逻辑重复 ⇒ 抽到 `lib/png.mjs` ✓）
  const image = decodePng(buf);
  const out = image.data;
  const bpp = image.channels;
  let red = 0;
  for (let i = 0; i < out.length; i += bpp) {
    if (out[i] > 150 && out[i + 1] < 100 && out[i + 2] < 100) red += 1;
  }
  // **期望值 64 是判据自己造的**（8×8）⇒ 可以写死 ✓
  check(red === W * H, `③ 导出的 PNG 里红块像素 = ${red}（应为 ${W * H}）`);
  try { rmSync(join(exportDir, outName)); } catch { /* 清理失败不影响判据 */ }
}

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（import_image 的像素必须真的出现）`);
  process.exit(1);
}
console.log("\n  ✓ import_image 导入的位图真的画出来了");
