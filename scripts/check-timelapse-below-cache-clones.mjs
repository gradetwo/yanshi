#!/usr/bin/env node
// **`below` 缓存不许每渲一次就整份拷贝** ✓（第 671 轮；那笔 27 倍提速的判据）
//
// **⇒ 背景**：一次 4608×8192 的散布重放，第一帧要 307 s（`YANSHI_RENDER_PROBE` 实测
//   `below缓存写` 占 233 s / 4856 次 ≈ 每次 48 ms ≈ 拷一份 37 MB 的表 ✗）。
//   修法是"同签名分组只写一次" ⇒ 同一夹具 **11.2 s** ✓。
// **⇒ 但计时跨机器不可比** ✗ ⇒ 所以这里断**结构量**：
//   程序现在会打印 `缓存账：below 克隆 N 条 tile（脏 tile M 条）` ✓
//   ⇒ **正常 N 远远小于 M**（≈ 签名数 × 一份表 ✓）。
//
// **⇒ 判据**：`N * 10 <= M`（即"克隆量不到脏 tile 的十分之一"）✓，且 `M > 10`（夹具有效 ✓）。
// **⇒ 变异**：把 `by_sig` 的插入改回**无条件** ⇒ 每渲一次拷一份 ⇒ `N ≈ M` ⇒ 红 ✓。
import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deflateSync } from "node:zlib";
import { createHash } from "node:crypto";
import { trackTemp } from "./lib/temp-dirs.mjs";

const BIN = "target/release/yanshi-timelapse";
for (const tool of ["tar", "ffmpeg", "ffprobe"]) {
  try {
    execFileSync("sh", ["-c", `command -v ${tool}`], { stdio: "ignore" });
  } catch {
    console.error(`  ⊘ 不可判：没有 ${tool}（这是环境前提，不是产品失败）`);
    process.exit(3);
  }
}

const DOC_W = 1024;
const DOC_H = 1024;
const IMG = 128;
const root = trackTemp(mkdtempSafe());
function mkdtempSafe() {
  const dir = join(tmpdir(), `tl-below-${process.pid}-${Date.now().toString(36)}`);
  mkdirSync(dir, { recursive: true });
  return dir;
}

const raw = Buffer.alloc(IMG * IMG * 4);
for (let i = 0; i < IMG * IMG; i += 1) {
  raw[i * 4] = 20;
  raw[i * 4 + 1] = 180;
  raw[i * 4 + 2] = 40;
  raw[i * 4 + 3] = 255;
}
const hex = createHash("sha256").update(raw).digest("hex");
const blobDir = join(root, "blobs", "sha256", hex.slice(0, 2), hex.slice(2, 4));
mkdirSync(blobDir, { recursive: true });
writeFileSync(join(blobDir, hex), deflateSync(raw));
writeFileSync(join(root, "blobs.encoding"), "zlib\n");

const atoms = [
  {
    id: "a0", seq: 1, kind: "create_document", actor: "human:below", session: "session:below",
    timestamp: 0, schema_version: 1,
    payload: { doc_id: "below", width: DOC_W, height: DOC_H, background: { r: 255, g: 255, b: 255, a: 255 }, color_space: "srgb" },
  },
  {
    id: "a1", seq: 2, kind: "create_layer", actor: "human:below", session: "session:below",
    timestamp: 0, schema_version: 1, payload: { layer_id: "L1", name: "L1", type: "raster" },
  },
];
// 6 张图散布 ⇒ 脏 tile 数够多（> 10 ✓），够让"每渲一次拷一份"露出来 ✓
const spots = [[40, 40], [520, 60], [60, 520], [540, 540], [260, 260], [700, 300]];
spots.forEach(([x, y], index) => {
  atoms.push({
    id: `img_${index}`, seq: 3 + index, kind: "import_image", actor: "human:below",
    session: "session:below", timestamp: 0, schema_version: 1,
    payload: {
      bitmap: { blob_hash: `sha256:${hex}`, mime_type: "image/x-yanshi-raw", size: raw.length },
      width: IMG, height: IMG, layer_id: "L1", object_id: `img${index}`,
      refs: { blobs: [`sha256:${hex}`] },
      region: { x, y, w: IMG, h: IMG },
      type: "raster_patch",
    },
  });
});
writeFileSync(join(root, "meta.json"), JSON.stringify({ width: DOC_W, height: DOC_H, doc_id: "below", fps: 24 }));
writeFileSync(join(root, "atoms.jsonl"), atoms.map((atom) => JSON.stringify(atom)).join("\n") + "\n");
execFileSync("tar", ["-cf", join(root, "probe.yanshi"), "-C", root, "meta.json", "atoms.jsonl", "blobs.encoding", "blobs"], {
  stdio: "ignore",
});

let text;
try {
  text = execFileSync("sh", ["-c", `${BIN} ${join(root, "probe.yanshi")} ${join(root, "out.mp4")} 1000 2>&1`], {
    encoding: "utf8", timeout: 300_000, maxBuffer: 32 * 1024 * 1024,
  });
} catch (error) {
  console.error(`  ✗ 跑不起来：${(error.stdout || error.stderr || "").slice(-400)}`);
  process.exit(1);
}
const cloned = /below 克隆 (\d+) 条 tile/.exec(text);
const dirty = /脏 tile (\d+) 条/.exec(text);
if (!cloned || !dirty) {
  console.error(`  ⊘ 不可判：输出里没有"缓存账"那一行 ⇒ 拿不到计数（不是产品失败）\n${text.slice(-400)}`);
  process.exit(3);
}
const cloneCount = Number(cloned[1]);
const dirtyTiles = Number(dirty[1]);
console.log(`  below 克隆 ${cloneCount} 条 tile｜脏 tile ${dirtyTiles} 条`);
if (dirtyTiles <= 10) {
  console.error(`  ⊘ 不可判：脏 tile 只有 ${dirtyTiles} 条 ⇒ 夹具太小，测不出"每渲一次拷一份"`);
  process.exit(3);
}
if (cloneCount * 10 > dirtyTiles) {
  console.error(
    `  ✗ below 克隆了 ${cloneCount} 条 tile，而脏 tile 只有 ${dirtyTiles} 条 ⇒ 缓存又在每渲一次整份拷贝（第 666 轮那个 bug 回来了）`,
  );
  process.exit(1);
}
console.log(`✓ below 缓存：克隆 ${cloneCount} 条 ≪ 脏 tile ${dirtyTiles} 条（同签名只写一次）`);
rmSync(root, { recursive: true, force: true });
