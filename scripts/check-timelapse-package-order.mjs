#!/usr/bin/env node
// **`.yanshi` 包的 `blobs.encoding` 允许出现在任意位置；解压失败必须响亮**（第 703 轮；外部审计 R-08）
//
// **⇒ 两个缺陷**：
// ① 单遍遍历 ⇒ 若 `blobs/sha256/*` 排在 `blobs.encoding` **前面**，那些 blob 会按初始值
//    `"none"` **不解压**就入库 ⇒ 画面拿 zlib 头当像素 ✗（tar 条目顺序**没有规范保证** ✗）。
// ② `zlib_decompress(...).unwrap_or_else(|| entry.bytes.clone())` ⇒ 解压失败**静默**把压缩字节
//    当原始像素入库 ⇒ 画面糊掉但**没有任何信号** ✗（本仓头号对手「静默降级」✓）。
//
// **⇒ 判据（两条）**：
// ① 把 `blobs.encoding` 放在**最后**打包 ⇒ 首帧左上角必须是**夹具的绿色**（不是 zlib 头的 `0x78`）✓；
// ② 声明 `zlib` 但内容损坏 ⇒ 二进制必须**非零退出**，且 stderr 提到解压失败 ✓。
//
// **变异**：把扫描改回单遍（或把 `ok_or_else` 改回 `unwrap_or_else`）⇒ ① 或 ② 红 ✓。
import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { deflateSync } from "node:zlib";
import { createHash } from "node:crypto";
import { trackTemp } from "./lib/temp-dirs.mjs";

const BIN = "target/release/yanshi-timelapse";
const unjudgeable = (why) => {
  console.error(`  ⊘ 不可判：${why}（不是产品问题）`);
  process.exit(3);
};
for (const tool of ["tar"]) {
  try {
    execFileSync("sh", ["-c", `command -v ${tool}`], { encoding: "utf8" });
  } catch {
    unjudgeable(`本机没有 ${tool}`);
  }
}

const W = 64;
const H = 64;
const GREEN = [20, 180, 40, 255];
const root = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-pkg-order-")));

const build = (name, { corrupt }) => {
  const dir = join(root, name);
  mkdirSync(join(dir, "blobs", "sha256", "aa", "bb"), { recursive: true });
  const rgba = Buffer.alloc(W * H * 4);
  for (let i = 0; i < W * H; i += 1) rgba.set(GREEN, i * 4);
  const hex = createHash("sha256").update(rgba).digest("hex");
  const blobDir = join(dir, "blobs", "sha256", hex.slice(0, 2), hex.slice(2, 4));
  mkdirSync(blobDir, { recursive: true });
  const payload = corrupt ? Buffer.from("not zlib at all") : deflateSync(rgba);
  writeFileSync(join(blobDir, hex), payload);
  writeFileSync(join(dir, "blobs.encoding"), "zlib\n");
  writeFileSync(join(dir, "meta.json"), JSON.stringify({ width: W, height: H, doc_id: "pkg", fps: 24 }));
  const atoms = [
    { id: "a0", seq: 1, kind: "create_document", actor: "human:t", session: "s", timestamp: 0, schema_version: 1,
      payload: { doc_id: "pkg", width: W, height: H, background: { r: 0, g: 0, b: 0, a: 255 }, color_space: "srgb" } },
    { id: "a1", seq: 2, kind: "create_layer", actor: "human:t", session: "s", timestamp: 0, schema_version: 1,
      payload: { layer_id: "L1", name: "L1", type: "raster" } },
    { id: "a2", seq: 3, kind: "import_image", actor: "human:t", session: "s", timestamp: 0, schema_version: 1,
      payload: {
        bitmap: { blob_hash: `sha256:${hex}`, mime_type: "image/x-yanshi-raw", size: rgba.length },
        width: W, height: H, layer_id: "L1", object_id: "img0",
        refs: { blobs: [`sha256:${hex}`] }, region: { x: 0, y: 0, w: W, h: H }, type: "raster_patch",
      } },
  ];
  writeFileSync(join(dir, "atoms.jsonl"), `${atoms.map((a) => JSON.stringify(a)).join("\n")}\n`);
  // **`blobs.encoding` 放在最后** ✓ —— 这正是缺陷 ① 的触发顺序 ✓
  const out = join(root, `${name}.yanshi`);
  execFileSync("tar", ["-cf", out, "-C", dir, "meta.json", "atoms.jsonl", "blobs", "blobs.encoding"], { stdio: "pipe" });
  return out;
};

const failures = [];

// ① 顺序无关：首帧左上角必须是夹具的绿色
{
  const pkg = build("ordered", { corrupt: false });
  const frames = join(root, "frames-ok");
  try {
    execFileSync("sh", ["-c", `${BIN} ${pkg} ${join(root, "ok.mp4")} 1000 2>&1`], {
      encoding: "utf8", timeout: 240_000, env: { ...process.env, YANSHI_TIMELAPSE_RAW_DIR: frames },
    });
  } catch (error) {
    failures.push(`① 跑不起来（${(error.stdout || error.stderr || "").slice(-200)}）`);
  }
  const files = readdirSync(frames).filter((name) => /^frame-\d+\.rgb$/.test(name)).sort();
  if (files.length === 0) {
    failures.push("① 一帧都没写出来");
  } else {
    const first = readFileSync(join(frames, files[0]));
    const pixel = [first[0], first[1], first[2]];
    const isGreen = Math.abs(pixel[0] - GREEN[0]) <= 2 && Math.abs(pixel[1] - GREEN[1]) <= 2 && Math.abs(pixel[2] - GREEN[2]) <= 2;
    if (!isGreen) {
      failures.push(`① 首帧左上角是 ${JSON.stringify(pixel)}，不是夹具的绿色 ${JSON.stringify(GREEN.slice(0, 3))} ⇒ \`blobs.encoding\` 排在后面时 blob 没被解压`);
    } else {
      console.log(`  ✓ ① \`blobs.encoding\` 排在最后也照样解压（首帧左上角 = ${JSON.stringify(pixel)}）`);
    }
  }
}

// ② 损坏的 zlib ⇒ 必须响亮失败
{
  const pkg = build("corrupt", { corrupt: true });
  let failed = false;
  let text = "";
  try {
    text = execFileSync("sh", ["-c", `${BIN} ${pkg} ${join(root, "bad.mp4")} 1000 2>&1`], {
      encoding: "utf8", timeout: 240_000, env: { ...process.env, YANSHI_TIMELAPSE_RAW_DIR: join(root, "frames-bad") },
    });
  } catch (error) {
    failed = true;
    text = `${error.stdout || ""}${error.stderr || ""}`;
  }
  if (!failed) {
    failures.push("② 损坏的 zlib 却**成功退出** ⇒ 静默把压缩字节当像素（外部审计 R-08 的第二半）");
  } else if (!/解压失败/.test(text)) {
    failures.push(`② 失败但没说清原因 ⇒ 用户看不出是包坏了：${text.slice(-200)}`);
  } else {
    console.log("  ✓ ② 损坏的 zlib ⇒ 非零退出且写明「解压失败」");
  }
}

if (failures.length > 0) {
  console.error(`  ✗ ${failures.length} 项没通过：`);
  for (const line of failures) console.error(`     - ${line}`);
  process.exit(1);
}
console.log("✓ 包顺序无关，且解压失败会响亮报错");
