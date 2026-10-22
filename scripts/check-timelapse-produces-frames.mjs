// **`yanshi-timelapse` 跑完必须产出一个 `ffprobe` 能读出 `nb_frames > 0` 的 mp4**（第 574 轮；外部第 86 次建议）。
//
// **为什么**：`f3bcde39` 之后外部实测「3m20s、**exit 0**、mp4 只有 **261 字节**、ffprobe 读不出流」✗
//   ⇒ **静默产出坏文件**是最坏的一种"成功" ✓ ⇒ 必须有判据守着 ✓。
//   （261 字节是我们文档里记过的"中途中止"指纹：ffmpeg 写完头就没了下文 ✓。）
//
// 做法：造一个**最小工程包**（裸 tar：`meta.json` ＋ `atoms.jsonl`）⇒ 真跑二进制 ⇒ 用 `ffprobe` 验帧数 ✓
//
// 退出码：0 = 出片且帧数 > 0｜1 = 出片失败／帧数为 0（**产品问题**）｜3 = 前提不成立（没二进制／没 ffmpeg／没 tar）
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const BINARY = "target/release/yanshi-timelapse";
const unjudgeable = (why) => {
  console.error(`⊘ 不可判：${why}（不是产品问题）`);
  process.exit(3);
};
const which = (name) => {
  try {
    return execFileSync("sh", ["-c", `command -v ${name}`], { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
};

if (!existsSync(BINARY)) unjudgeable(`没有 ${BINARY}（先 cargo build --release -p yanshi-timelapse）`);
if (!which("ffprobe")) unjudgeable("没有 ffprobe");
if (!which("tar")) unjudgeable("没有 tar");

const root = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-timelapse-check-")));
const meta = { width: 128, height: 96, doc_id: "tl_check", fps: 24 };
const atoms = [
  {
    id: "a0",
    seq: 1,
    kind: "create_document",
    actor: "human:check",
    session: "session:check",
    timestamp: 0,
    schema_version: 1,
    payload: {
      doc_id: "tl_check",
      width: 128,
      height: 96,
      background: { r: 255, g: 255, b: 255, a: 255 },
      color_space: "srgb",
    },
  },
  {
    id: "a1",
    seq: 2,
    kind: "create_layer",
    actor: "human:check",
    session: "session:check",
    timestamp: 0,
    schema_version: 1,
    payload: { layer_id: "L1", name: "L1" },
  },
  ...Array.from({ length: 6 }, (_, index) => ({
    id: `b${index}`,
    seq: 3 + index,
    kind: "create_object",
    actor: "human:check",
    session: "session:check",
    timestamp: 0,
    schema_version: 1,
    payload: {
      object_id: `s${index}`,
      layer_id: "L1",
      kind: "shape",
      data: {
        geometry: { kind: "rect", bbox: { x: 8 + index * 16, y: 16, w: 12, h: 12 } },
        color: { r: 10, g: 200, b: 30, a: 255 },
      },
    },
  })),
];

writeFileSync(join(root, "meta.json"), JSON.stringify(meta));
writeFileSync(join(root, "atoms.jsonl"), `${atoms.map((atom) => JSON.stringify(atom)).join("\n")}\n`);
execFileSync("tar", ["-cf", join(root, "probe.yanshi"), "-C", root, "meta.json", "atoms.jsonl"], {
  stdio: "pipe",
});

let run;
try {
  run = execFileSync("sh", ["-c", `${BINARY} ${join(root, "probe.yanshi")} ${join(root, "out.mp4")} 1 2>&1`], {
    encoding: "utf8",
    timeout: 240_000,
  });
} catch (error) {
  console.error(`✗ 二进制运行失败（退出码非零）⇒ 产品问题：\n${(error.stdout || "").slice(-800)}`);
  process.exit(1);
}

const probeOutput = (() => {
  try {
    return execFileSync("ffprobe", ["-v", "error", "-show_entries", "stream=nb_frames", "-of", "csv", join(root, "out.mp4")], {
      encoding: "utf8",
      timeout: 60_000,
    });
  } catch (error) {
    console.error(`✗ ffprobe 读不出流 ⇒ 产出了坏文件：${(error.stderr || "").slice(-400)}\n${(run || "").slice(-400)}`);
    process.exit(1);
  }
})();

const frames = probeOutput
  .split("\n")
  .map((line) => line.trim().split(","))
  .filter((parts) => parts.length >= 2 && parts[0] === "stream")
  .map((parts) => Number(parts[1]))
  .filter((value) => Number.isFinite(value));

if (frames.length === 0) {
  console.error(`✗ ffprobe 没给出 nb_frames ⇒ 判不了帧数：\n${probeOutput.trim()}`);
  process.exit(1);
}
const total = Math.max(...frames);
if (total <= 0) {
  console.error(`✗ mp4 里 nb_frames=${total} ⇒ 出的是空片（外部第 86 次就是 261 字节全是头）\n${(run || "").slice(-400)}`);
  process.exit(1);
}
console.log(`✓ timelapse 出片：${total} 帧（最小工程包 128×96、7 条原子）`);
