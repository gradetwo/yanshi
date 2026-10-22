#!/usr/bin/env node
// **导出出来的像素，必须和渲染出来的像素是同一个颜色** ✓（第 608 轮，服务色彩 Phase 1）
//
// **⇒ 为什么需要它**：色彩体系统一（Phase 1）最怕的就是**同一条管线里转两次**
//   —— 例如渲染时 sRGB→linear、导出时又转一次 ⇒ 画面会**整体偏移**，而且**很难被现有判据发现**
//   （`tool-export-deterministic` 只比"两次导出是否相同" ⇒ **两次都错也一样相同** ✓）。
//
// **⇒ 这条判据做三件事**：
//   ① 同一块区域，`render_region(raw)` 的像素 **== `export_png` 解码出来的像素** ✓
//   ② 两者都必须**等于所填颜色**（`{r:10,g:200,b:30}`）✓
//   ③ 差异若超过 ±1 就红（PNG 是无损的 ⇒ **只允许 0 差**；留 1 是给未来 rounding 的余地）
//
// **⇒ 变异**：把导出路径里的颜色转换改成"再转一次"⇒ ① 红 ✓；
//   把期望值写成别的颜色 ⇒ ② 红 ✓（证明断言真的在读那条路径）
import { existsSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-matches-render.mjs <base> <doc> <token>");
  process.exit(2);
}
// **导出文件的路径以工具返回的 `path` 为准** ✓（第 610 轮 CI 实测）：
//   原版只认 `YANSHI_EXPORT_DIR` ⇒ **CI 上它没设** ⇒ 判据 `exit 3`「前置条件缺失」✗
//   ⇒ 那就是「只在本机能过」的判据 ✓ ⇒ 现在**优先用响应里的 `path`**（实测存在且是绝对路径 ✓），
//     环境变量只作为老服务器的兜底 ✓。
const exportDir = process.env.YANSHI_EXPORT_DIR ?? null;

const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const check = (ok, message) => {
  if (!ok) {
    console.error(`  ✗ ${message}`);
    process.exit(1);
  }
};

// **填色与期望值必须分开写** ✓（第 608 轮实测）：我第一版用同一个常量既填又比 ⇒
//   把常量改成 `g:201` 时**填的也跟着变** ⇒ 判据照样绿 ✗ ⇒ **变异证明不了它在读那条路径** ✓
// **三个亮度都要查** ✓（第 614 轮）：**二次 sRGB↔linear 转换在中间调最明显** ✓
//   —— 深色几乎不变、亮色接近饱和、**128 会掉 60+ 级** ⇒ 只测一个亮色会漏掉它 ✗
// **`fill` 与 `want` 必须分开** ✓（第 614 轮）：写在一起的常量**变异不红** ✗（第 608 轮踩过）
const SAMPLES = [
  { fill: { r: 10, g: 200, b: 30 }, want: { r: 10, g: 200, b: 30 } },    // 深绿
  { fill: { r: 128, g: 128, b: 128 }, want: { r: 128, g: 128, b: 128 } }, // **中灰（伽马错最敏感）**
  { fill: { r: 250, g: 250, b: 250 }, want: { r: 250, g: 250, b: 250 } }, // 接近白
];
const PNG = `export_vs_render_${Date.now().toString(36)}.png`;

// **图层可能已经存在** ✓（跑在 `_bootstrap` 上时它本来就有 `layer_default`）
//   ⇒ "已存在"是**前提已满足** ✓，不是失败 ✓
const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
const layerDetail = String(layer.context?.detail ?? "");
check(
  layer.ok === true || layerDetail.includes("已存在"),
  `create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`,
);

// **三条带子各填一次** ✓（第 614 轮）：每条 64×20，采样点取各自中心 ✓
const bands = [];
for (const [index, sample] of SAMPLES.entries()) {
  const y = index * 20;
  const filled = await call("fill_region", {
    layer_id: "layer_default",
    shape: { type: "rect", x: 0, y, w: 64, h: 20 },
    color: { ...sample.fill, a: 255 },
    opacity: 1.0,
  });
  check(filled.ok === true, `fill_region（第 ${index + 1} 条）⇒ ${JSON.stringify(filled).slice(0, 140)}`);
  bands.push({ ...sample, y: y + 10 });
}

// ① 渲染像素（每条带子各取一点）
const renderedPixels = [];
for (const band of bands) {
  const raw = await call("render_region", { region: { x: 32, y: band.y, w: 1, h: 1 }, raw: true, max_px: 1000 });
  check(raw.ok === true && Boolean(raw.raw_url), `render_region(raw) y=${band.y} ⇒ ${JSON.stringify(raw).slice(0, 140)}`);
  const rendered = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
  renderedPixels.push([rendered[0], rendered[1], rendered[2], rendered[3]]);
}

// ② 导出像素（PNG 无损 ⇒ 用 ffmpeg 解成 rgba）
const exported = await call("export_png", { path: PNG, deterministic: true });
check(exported.ok === true, `export_png ⇒ ${JSON.stringify(exported).slice(0, 140)}`);
// 优先级：响应里的 `path` ✓ → `YANSHI_EXPORT_DIR` 兜底 ✓ → 不可判
const candidates = [];
if (typeof exported.path === "string" && exported.path.length > 0) candidates.push(exported.path);
if (exportDir) candidates.push(join(exportDir, PNG));
const file = candidates.find((candidate) => existsSync(candidate));
if (!file) {
  console.error(
    `  ⊘ 不可判：拿不到导出的 PNG（响应 path=${JSON.stringify(exported.path ?? null)}｜YANSHI_EXPORT_DIR=${JSON.stringify(exportDir)}）——不是产品问题`,
  );
  process.exit(3);
}
let decoded;
try {
  decoded = execFileSync("ffmpeg", ["-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgba", "-"], {
    timeout: 60_000,
    maxBuffer: 64 * 1024 * 1024,
  });
} catch (error) {
  console.error(`  ⊘ 不可判：ffmpeg 解不出这个 PNG：${(error.stderr || "").toString().slice(-200)}`);
  process.exit(3);
}
const width = exported.width ?? 64;
const near = (a, b) => Math.abs(a - b) <= 1;
const exact = (a, b) => a === b;

for (const [index, band] of bands.entries()) {
  const at = (band.y * width + 32) * 4;
  const exportedPixel = [decoded[at], decoded[at + 1], decoded[at + 2], decoded[at + 3]];
  const renderedPixel = renderedPixels[index];
  console.log(
    `  第 ${index + 1} 条（${JSON.stringify(band.fill)}）渲染 ${JSON.stringify(renderedPixel)}｜导出 ${JSON.stringify(exportedPixel)}`,
  );
  check(
    near(renderedPixel[0], exportedPixel[0]) &&
      near(renderedPixel[1], exportedPixel[1]) &&
      near(renderedPixel[2], exportedPixel[2]) &&
      near(renderedPixel[3], exportedPixel[3]),
    `第 ${index + 1} 条的导出像素与渲染像素不一致 ⇒ 导出路径可能又转了一次颜色（渲染 ${JSON.stringify(renderedPixel)} vs 导出 ${JSON.stringify(exportedPixel)}）`,
  );
  // **与所填颜色必须是精确相等** ✓（容差 1 会让"期望值差 1"的变异漏过 —— 第 608 轮实测）
  check(
    exact(renderedPixel[0], band.want.r) && exact(renderedPixel[1], band.want.g) && exact(renderedPixel[2], band.want.b),
    `第 ${index + 1} 条的渲染像素不等于所填颜色 ${JSON.stringify(band.want)} ⇒ 颜色在渲染路径里被改了：${JSON.stringify(renderedPixel)}`,
  );
}
console.log(`  ✓ ${bands.length} 条带子：导出像素 == 渲染像素 == 所填颜色 ⇒ 导出路径没有二次颜色转换`);
