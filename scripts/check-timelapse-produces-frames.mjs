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
      type: "shape",
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
// **帧数 > 0 还不够 —— 画面必须有内容** ✓（第 585 轮实测：合成包曾因原子写错而**全白**，
//   而"帧数 > 0"照样通过 ✗）⇒ 把**所有帧**解成 rawvideo，要求**至少一帧**有非背景色像素 ✓。
//   ⚠️ 不要用 `-sseof` 取"最后一帧"：实测那次取到的是**空白**（误导）✗ —— 全解一遍最稳 ✓。
let raw;
try {
  raw = execFileSync(
    "ffmpeg",
    ["-v", "error", "-i", join(root, "out.mp4"), "-f", "rawvideo", "-pix_fmt", "rgb24", "-"],
    { timeout: 120_000, maxBuffer: 256 * 1024 * 1024 },
  );
} catch (error) {
  console.error(`✗ 解不出帧：${(error.stderr || "").toString().slice(-400)}`);
  process.exit(1);
}
const frameSize = meta.width * meta.height * 3;
const decoded = Math.floor(raw.length / frameSize);
let painted = 0;
for (let index = 0; index < decoded; index += 1) {
  const slice = raw.subarray(index * frameSize, (index + 1) * frameSize);
  if ([...slice].some((byte) => byte !== 255)) painted += 1;
}
if (painted === 0) {
  console.error(`✗ 解出 ${decoded} 帧，但没有一帧有内容（全背景色）⇒ 视频是空的：帧数对而画面空白`);
  process.exit(1);
}
// **颜色也要对** ✓（第 588 轮）：画的是 `{r:10,g:200,b:30}` ⇒ **画出来的像素必须绿占优** ✓
//   ⇒ 这能抓到"整条管线颜色被换掉"（色彩空间、通道序、预乘）那类错误 ✓（视频编码有 YUV 往返 ⇒ 只看**相对**关系 ✓）。
let green = 0;
let reddish = 0;
let blues = 0;
for (let index = 0; index < decoded; index += 1) {
  const slice = raw.subarray(index * frameSize, (index + 1) * frameSize);
  for (let at = 0; at + 2 < slice.length; at += 3) {
    const [r, g, b] = [slice[at], slice[at + 1], slice[at + 2]];
    if (r === 255 && g === 255 && b === 255) continue;
    if (g > r && g > b) green += 1;
    else if (r > g && r > b) reddish += 1;
    else if (b > g && b > r) blues += 1;
  }
}
// **必须是"多数派"** ✓（只看 >0 会被压缩噪点蒙混过关：实测把夹具画成红色后，仍有 264 个"绿占优"噪点 ✗）
if (!(green > reddish && green > blues)) {
  console.error(
    `✗ 画的是绿色 {10,200,30}，但像素里不是绿占优（绿 ${green}｜红占优 ${reddish}｜蓝占优 ${blues}）⇒ 颜色不对`,
  );
  process.exit(1);
}
// **进度行必须出现在 stderr 上** ✓（第 611 轮；外部第 91 次：`timeout` 杀掉后**一行诊断都没有**）
if (!String(run).includes("进度 ")) {
  console.error(`✗ stderr 里没有进度行 ⇒ 被 timeout 杀掉时无法判断卡在哪一段\n${String(run).slice(-400)}`);
  process.exit(1);
}


// **读包耗时也要报** ✓（第 612 轮；外部第 91 次要"阶段计时"）
if (!String(run).includes("读包 ")) {
  console.error(`✗ stderr 里没有读包耗时 ⇒ 无法区分"读包慢"与"渲染慢"\n${String(run).slice(-400)}`);
  process.exit(1);
}
console.log(
  `✓ timelapse 出片：${total} 帧，其中 ${painted}/${decoded} 帧有内容，且颜色对（绿占优像素 ${green} 个，` +
    `红占优 ${reddish}／蓝占优 ${blues}）（最小工程包 128×96、7 条原子）`,
);
