#!/usr/bin/env node
// **散布型夹具**：8 个图层、每层一张图，分散在画布四角与四边 ✓（第 646 轮；外部第 96 次的建议 ✓）
//
// **⇒ 为什么需要它**：外部第 96 次指出 —— 我上一版判据的夹具是**聚集型**（一张图），
//   而他们的包是**散布型**（200 张 `import_image` 分散在 `4608×8192` 上，8 个图层）
//   ⇒ 他们怀疑"降采样回放只降缩放、不降脏区"，对我这种形状可能无效 ✓
//   ⇒ 所以这条判据用**散布型**：8 个图层、8 张图分别贴在四角与四边中点 ✓
//
// **⇒ 背景**：外部第 93/94 次的包是 `4608×8192` 出 `608×1080` ⇒ 渲 37.7 M px 只为 0.66 M px（**57× 白算**）✗
//   ⇒ 降采样回放（在输出尺寸上回放）是解法 ✓ —— 但**缩错会静默丢掉内容** ✗
//   （第 634 轮实测：`tiles` 索引没换、`blob_hash` 前缀没剥、宽高取错位置 —— 三个都会让画面只剩背景 ✓）
//
// **⇒ 夹具形状必须照抄真实包**（否则量到的是假象 —— 我为此白跑了三轮 ✗）：
//   `payload`: {bitmap:{blob_hash:"sha256:<hex>", mime_type, size}, width, height, region, refs.blobs, type:"raster_patch"}
//   包里 blob 路径**分片**：`blobs/sha256/<h0h1>/<h2h3>/<hex>` ✓
//
// **⇒ 判据两条**：
//   ① **结构量**：缩之后脏区 tile **严格少于**不缩（不靠计时 ⇒ 跨机器可比 ✓）；
//   ② **内容**：两种模式下**末帧的颜色种数都 > 1**（贴着的那张图有 4 种颜色 ✓），
//      且**缩后的种数 ≥ 不缩的一半**（缩会混色，但不该塌成纯色 ✓）。
//
// **变异**：把 `YANSHI_TIMELAPSE_SHRINK` 的支持去掉（或让 `shrink_atom` 恒返回 `None`）⇒ ① 不成立 ⇒ 红 ✓。
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import zlib from "node:zlib";
import { createHash } from "node:crypto";
import { trackTemp } from "./lib/temp-dirs.mjs";

const root = trackTemp(mkdtempSync(join(tmpdir(), "timelapse-shrink-")));
mkdirSync(join(root, "blobs/sha256"), { recursive: true });
const BIN = "target/release/yanshi-timelapse";
for (const tool of ["tar", "ffmpeg", "ffprobe"]) {
  try {
    execFileSync("sh", ["-c", `command -v ${tool}`], { encoding: "utf8" });
  } catch {
    console.error(`  ⊘ 不可判：本机没有 ${tool}（不是产品问题）`);
    process.exit(3);
  }
}

const DOC_W = 4608;
const DOC_H = 8192;
const IMG = 64; // 64×64、四种颜色 ⇒ 缩放后仍能数出 >1 种 ✓
const COLOURS = [
  [10, 200, 30, 255],
  [200, 30, 30, 255],
  [30, 30, 200, 255],
  [250, 250, 30, 255],
];
const raw = Buffer.alloc(IMG * IMG * 4);
for (let i = 0; i < IMG * IMG; i += 1) raw.set(COLOURS[i % COLOURS.length], i * 4);
const blob = zlib.deflateSync(raw);
const hex = createHash("sha256").update(raw).digest("hex");
const sharded = `blobs/sha256/${hex.slice(0, 2)}/${hex.slice(2, 4)}/${hex}`;
mkdirSync(join(root, `blobs/sha256/${hex.slice(0, 2)}/${hex.slice(2, 4)}`), { recursive: true });
writeFileSync(join(root, sharded), blob);
writeFileSync(join(root, "blobs.encoding"), "zlib\n");

// **8 个图层，每层一张图，贴在四角与四边中点** ✓
const CORNERS = [
  [40, 40],
  [DOC_W - IMG - 40, 40],
  [40, DOC_H - IMG - 40],
  [DOC_W - IMG - 40, DOC_H - IMG - 40],
  [Math.round(DOC_W / 2 - IMG / 2), 40],
  [Math.round(DOC_W / 2 - IMG / 2), DOC_H - IMG - 40],
  [40, Math.round(DOC_H / 2 - IMG / 2)],
  [DOC_W - IMG - 40, Math.round(DOC_H / 2 - IMG / 2)],
];
const atoms = [
  {
    id: "a0", seq: 1, kind: "create_document", actor: "human:shrink", session: "session:shrink",
    timestamp: 0, schema_version: 1,
    payload: { doc_id: "shrink", width: DOC_W, height: DOC_H, background: { r: 255, g: 255, b: 255, a: 255 }, color_space: "srgb" },
  },
];
CORNERS.forEach(([x, y], index) => {
  const layerId = `L${index + 1}`;
  atoms.push({
    id: `layer_${index}`, seq: 2 + index * 2, kind: "create_layer", actor: "human:shrink",
    session: "session:shrink", timestamp: 0, schema_version: 1,
    payload: { layer_id: layerId, name: layerId, type: "raster" },
  });
  atoms.push({
    id: `img_${index}`, seq: 3 + index * 2, kind: "import_image", actor: "human:shrink",
    session: "session:shrink", timestamp: 0, schema_version: 1,
    payload: {
      bitmap: { blob_hash: `sha256:${hex}`, mime_type: "image/x-yanshi-raw", size: raw.length },
      width: IMG, height: IMG, layer_id: layerId, object_id: `img${index}`,
      refs: { blobs: [`sha256:${hex}`] },
      region: { x, y, w: IMG, h: IMG },
      type: "raster_patch",
    },
  });
});
writeFileSync(join(root, "meta.json"), JSON.stringify({ width: DOC_W, height: DOC_H, doc_id: "shrink", fps: 24 }));
writeFileSync(join(root, "atoms.jsonl"), atoms.map((a) => JSON.stringify(a)).join("\n") + "\n");
execFileSync("tar", ["-cf", join(root, "probe.yanshi"), "-C", root, "meta.json", "atoms.jsonl", "blobs.encoding", "blobs"], {
  timeout: 60_000,
});

const framesOf = (file, w, h) =>
  execFileSync("ffmpeg", ["-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], {
    timeout: 180_000, maxBuffer: 512 * 1024 * 1024,
  }).length / (w * h * 3);

const distinctInLastFrame = (file, w, h) => {
  const bytes = execFileSync("ffmpeg", ["-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], {
    timeout: 180_000, maxBuffer: 512 * 1024 * 1024,
  });
  const size = w * h * 3;
  const count = Math.floor(bytes.length / size);
  if (count === 0) return 0;
  const last = bytes.subarray((count - 1) * size);
  const seen = new Set();
  for (let at = 0; at + 2 < last.length; at += 3) {
    seen.add((last[at] << 16) | (last[at + 1] << 8) | last[at + 2]);
  }
  return seen.size;
};

const runOnce = (shrink) => {
  const out = join(root, shrink ? "shrunk.mp4" : "plain.mp4");
  rmSync(out, { force: true });
  const env = { ...process.env };
  if (shrink) env.YANSHI_TIMELAPSE_SHRINK = "1";
  else delete env.YANSHI_TIMELAPSE_SHRINK;
  let text;
  try {
    // **每 1000 个 atom 一帧 ⇒ 只出 1 帧** ✓（散布型夹具在 4608×8192 上每帧要 ~27 s ✗ ⇒ 只验"这一帧的脏区"）
    text = execFileSync("sh", ["-c", `${BIN} ${join(root, "probe.yanshi")} ${out} 1000 2>&1`], {
      encoding: "utf8", timeout: 300_000, env, maxBuffer: 32 * 1024 * 1024,
    });
  } catch (error) {
    console.error(`  ✗ 跑不起来（shrink=${shrink}）：${(error.stdout || error.stderr || "").slice(-400)}`);
    process.exit(1);
  }
  const tiles = /脏区 \d+ 次（(\d+) tile/.exec(text);
  const dims = /输出 (\d+)×(\d+)/.exec(text);
  const width = dims ? Number(dims[1]) : 0;
  const height = dims ? Number(dims[2]) : 0;
  const frames = width > 0 ? framesOf(out, width, height) : 0;
  const colours = width > 0 ? distinctInLastFrame(out, width, height) : 0;
  return { text, tiles: tiles ? Number(tiles[1]) : -1, frames, colours, width, height };
};

const plain = runOnce(false);
const shrunk = runOnce(true);
// **横幅必须出现** ✓：证明降采样回放**真的启用**了（而不是被安全阀退回）
if (!shrunk.text.includes("降采样回放：")) {
  console.error("  ✗ 缩的那次没有打出「降采样回放」横幅 ⇒ 整包被退回原尺寸（安全阀）");
  process.exit(1);
}
console.log(`  不缩：${plain.width}×${plain.height}｜脏区 ${plain.tiles} tile｜${plain.frames} 帧｜末帧 ${plain.colours} 色`);
console.log(`  缩：  ${shrunk.width}×${shrunk.height}｜脏区 ${shrunk.tiles} tile｜${shrunk.frames} 帧｜末帧 ${shrunk.colours} 色`);

if (plain.tiles < 0 || shrunk.tiles < 0) {
  console.error("  ⊘ 不可判：没解析到渲染账（判据要靠它拿 tile 数）");
  process.exit(3);
}
// ① 结构量：缩之后必须真的少渲
if (!(shrunk.tiles < plain.tiles)) {
  console.error(`  ✗ 缩之后脏区 tile 没变少（不缩 ${plain.tiles} vs 缩 ${shrunk.tiles}）⇒ 降采样回放没生效`);
  process.exit(1);
}
// ② 内容：两种模式的末帧都必须有内容，且缩后不许塌成纯色
if (plain.colours <= 1) {
  console.error(`  ✗ 不缩的末帧只有 ${plain.colours} 种颜色 ⇒ 夹具本身没画上内容（不是产品问题，先修夹具）`);
  process.exit(3);
}
if (shrunk.colours <= 1) {
  console.error(`  ✗ 缩之后的末帧只有 ${shrunk.colours} 种颜色 ⇒ 内容被丢了（第 634 轮那三个坑）`);
  process.exit(1);
}
if (shrunk.colours * 2 < plain.colours) {
  console.error(
    `  ✗ 缩之后颜色种数掉了一半以上（不缩 ${plain.colours} vs 缩 ${shrunk.colours}）⇒ 画面明显变坏`,
  );
  process.exit(1);
}
console.log(
  `✓ 降采样回放：脏区 ${plain.tiles} → ${shrunk.tiles} tile，末帧颜色 ${plain.colours} → ${shrunk.colours} 种（内容保住）`,
);
