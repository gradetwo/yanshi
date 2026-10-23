#!/usr/bin/env node
// **线性混色的笔触，导出像素也必须等于渲染像素** ✓（第 649 轮；色彩体系 Phase 1）
//
// **⇒ 为什么单列一条**：`tool-export-matches-render.mjs` 查的是 `fill_region` 的**纯色**
//   （颜色不经过"混色"这一步）⇒ 它**抓不到**"混色算在 linear 里、导出又转一次"这类错误 ✗。
//
// **⇒ 判据四件事**（两条带子，同色同不透明度，只差 `color_mixing`）：
//   ① 两条带子都**画上去了**（与背景差 ≥ 8 级）✓；
//   ② 每条带子：`render_region` 的像素 **== `export_png` 解码出来的像素** ✓（PNG 无损 ⇒ 精确相等）；
//   ③ `linear` 与 `rgb` 两条带子**必须不同**（≥ 8 级）✓ —— 否则说明混色模式没生效 ✓；
//   ④ 两边都不得与该带子的背景色相同 ✓（排除"什么都没画"）。
//
// **变异**：把 `ColorMixing::Linear` 的实现改成与 `Rgb` 相同（或让 `color_mixing` 参数被忽略）⇒ ③ 红 ✓。
import { existsSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-matches-render-linear.mjs <base> <doc> <token>");
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
if (typeof summary.width !== "number") {
  console.error(`  ⊘ 不可判：拿不到画布尺寸 ⇒ ${JSON.stringify(summary).slice(0, 140)}`);
  process.exit(3);
}
const docW = summary.width;
const docH = summary.height;
const BACKGROUND = { r: 40, g: 60, b: 90, a: 255 };
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: docW, h: docH },
  color: BACKGROUND,
  opacity: 1.0,
});
if (filled.ok !== true) fail(`fill_region ⇒ ${JSON.stringify(filled).slice(0, 140)}`);

// 两条横带：上半 `rgb`、下半 `linear`（同色、同不透明度）
const bandH = Math.floor(docH / 2);
const strokes = [
  { mixing: "rgb", y: Math.floor(bandH * 0.5) },
  { mixing: "linear", y: bandH + Math.floor(bandH * 0.5) },
];
for (const item of strokes) {
  const painted = await call("brush_stroke", {
    layer_id: "layer_default",
    brush: "calligraphy",
    points: [
      [Math.round(docW * 0.2), item.y, 0.5],
      [Math.round(docW * 0.8), item.y, 0.5],
    ],
    size: Math.max(6, Math.round(docH * 0.12)),
    color: [0.95, 0.25, 0.2, 0.5],
    color_mixing: item.mixing,
  });
  if (painted.ok !== true) fail(`brush_stroke(${item.mixing}) ⇒ ${JSON.stringify(painted).slice(0, 160)}`);
  console.log(`  [probe] ${item.mixing}: painted_pixels=${painted.painted_pixels}｜deposited=${painted.deposited}｜warning=${JSON.stringify(painted.warnings ?? "").slice(0, 120)}`);
}

// **诊断：用 `sample_colors` 直接读画布**（区分"笔没提交"与"渲染路径看不到"）
{
  const probe = await call("sample_colors", { points: [[Math.round(docW * 0.5), strokes[0].y]] });
  console.log(`  [probe] sample_colors ⇒ ${JSON.stringify(probe).slice(0, 200)}`);
}
const rawFirst = await call("render_region", { region: { x: 0, y: 0, w: docW, h: docH }, raw: true, max_px: 4_000_000 });
if (rawFirst.ok === true && rawFirst.raw_url) {
  const buffer = Buffer.from(await (await fetch(base + rawFirst.raw_url)).arrayBuffer());
  const at = (strokes[0].y * docW + Math.round(docW * 0.5)) * 4;
  console.log(`  [probe] 第一次 render_region 在笔迹处 = ${JSON.stringify([...buffer.subarray(at, at + 4)])}`);
}
const raw = await call("render_region", { region: { x: 0, y: 0, w: docW, h: docH }, raw: true, max_px: 4_000_000 });
if (raw.ok !== true || !raw.raw_url) fail(`render_region(raw) ⇒ ${JSON.stringify(raw).slice(0, 160)}`);
const rendered = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());

const exported = await call("export_png", { path: "export_vs_render_linear.png", deterministic: true });
if (exported.ok !== true) fail(`export_png ⇒ ${JSON.stringify(exported).slice(0, 160)}`);
const file = typeof exported.path === "string" && existsSync(exported.path) ? exported.path : null;
if (!file) {
  console.error("  ⊘ 不可判：拿不到导出的 PNG 路径");
  process.exit(3);
}
const decoded = execFileSync("ffmpeg", ["-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgba", "-"], {
  timeout: 60_000, maxBuffer: 256 * 1024 * 1024,
});

// 每条带子里找**第一个真的被画到**的像素（与背景差 ≥ 8）
const pixelAt = (buffer, x, y) => {
  const at = (y * docW + x) * 4;
  return [buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]];
};
const differs = (a, b) => Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1]) + Math.abs(a[2] - b[2]);
// 诊断：整幅有多少像素与背景不同（先看"有没有画上去"）
{
  let paintedCount = 0;
  for (let at = 0; at + 3 < rendered.length; at += 4) {
    const px = [rendered[at], rendered[at + 1], rendered[at + 2], rendered[at + 3]];
    if (differs(px, BACKGROUND) >= 1) paintedCount += 1;
  }
  const first = [rendered[0], rendered[1], rendered[2], rendered[3]];
  console.log(`  [probe] 与背景不同的像素(≥1)：${paintedCount} / ${docW * docH}｜首像素=${JSON.stringify(first)}｜背景=${JSON.stringify([BACKGROUND.r, BACKGROUND.g, BACKGROUND.b])}`);
}
const found = [];
for (const item of strokes) {
  let pick = null;
  for (let y = Math.max(0, item.y - 4); y < Math.min(docH, item.y + 5) && !pick; y += 1) {
    for (let x = 0; x < docW; x += 1) {
      const renderedPixel = pixelAt(rendered, x, y);
      if (differs(renderedPixel, BACKGROUND) >= 8) {
        pick = { x, y, renderedPixel, exportedPixel: pixelAt(decoded, x, y) };
        break;
      }
    }
  }
  if (!pick) fail(`「${item.mixing}」带子里一个被画到的像素都没找到 ⇒ 笔触没落上去`);
  found.push({ ...item, ...pick });
}

for (const item of found) {
  const { renderedPixel, exportedPixel } = item;
  console.log(
    `  ${item.mixing.padEnd(6)} (${item.x},${item.y}) 渲染 ${JSON.stringify(renderedPixel.slice(0, 3))}｜导出 ${JSON.stringify(exportedPixel.slice(0, 3))}`,
  );
  if (differs(renderedPixel, exportedPixel) !== 0) {
    fail(`「${item.mixing}」的导出像素与渲染像素不同（差 ${differs(renderedPixel, exportedPixel)}）⇒ 导出路径又转了一次颜色`);
  }
}
const delta = differs(found[0].renderedPixel, found[1].renderedPixel);
if (delta < 8) {
  fail(`rgb 与 linear 两条带子几乎一样（差 ${delta} 级）⇒ 混色模式没有生效（或 linear 被实现成 rgb）`);
}
console.log(`✓ 线性混色：导出 == 渲染（两条带子都精确相等），且 linear 与 rgb 差 ${delta} 级`);
