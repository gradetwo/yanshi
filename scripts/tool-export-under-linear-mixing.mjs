#!/usr/bin/env node
// **混过色的像素，导出必须等于渲染；且 linear 与 rgb 必须不同** ✓（第 653 轮；色彩 Phase 1）
//
// **⇒ 为什么这样造**：第 652 轮实测 —— **一笔不透明的笔**完全覆盖背景 ⇒ 混色模式**看不出差别** ✗。
//   这里照 `tool-color-mixing.mjs` 的做法：**同一处先黄后蓝两笔** ⇒ 蓝色笔要**和已有的黄混**
//   ⇒ 混色模式才真的参与计算 ✓。
//
// **⇒ 判据三条**：
//   ① 两条带子重叠处的像素：`render_region` **精确等于** `export_png` 解出来的 ✓；
//   ② `linear` 与 `rgb` 两条带子的重叠像素**必须不同**（差 ≥ 8）✓；
//   ③ 两条带子都必须**真的被画到**（与背景差 ≥ 8）✓。
//
// **变异**：把 `ColorMixing::Linear` 实现成与 `Rgb` 相同 ⇒ ② 红 ✓。
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-under-linear-mixing.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}) });
  return response.json();
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};
// **颜色一律用数组** ✓（第 652 轮的教训：对象下标是 undefined ⇒ 差值 NaN ⇒ 恒 false）
const BACKGROUND = [128, 128, 128];
const distance = (left, right) =>
  Math.abs(left[0] - right[0]) + Math.abs(left[1] - right[1]) + Math.abs(left[2] - right[2]);

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
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: docW, h: docH },
  color: { r: BACKGROUND[0], g: BACKGROUND[1], b: BACKGROUND[2], a: 255 },
  opacity: 1.0,
});
if (filled.ok !== true) fail(`fill_region ⇒ ${JSON.stringify(filled).slice(0, 140)}`);

const bandH = Math.floor(docH / 2);
const bands = [
  { mixing: "rgb", y: Math.floor(bandH * 0.5) },
  { mixing: "linear", y: bandH + Math.floor(bandH * 0.5) },
];
const x0 = Math.round(docW * 0.2);
const x1 = Math.round(docW * 0.8);
const size = Math.max(8, Math.round(docH * 0.3));
for (const band of bands) {
  // **一笔渐变（黄 → 蓝）** ✓：照 `tool-color-mixing.mjs` 的做法 —— 混色发生在**渐变中段** ✓
  //   （我前两版用"两笔叠加"，但后一笔**完全覆盖**前一笔 ⇒ 只剩纯蓝，测不到混色 ✗）
  const drawn = await call("brush_stroke", {
    layer_id: "layer_default",
    brush: "100%_Opaque",
    size,
    opacity: 1.0,
    color: [1.0, 1.0, 0.0, 1.0],
    color_to: [0.0, 0.0, 1.0, 1.0],
    points: [[x0, band.y, 0.5], [Math.round((x0 + x1) / 2), band.y, 0.5], [x1, band.y, 0.5]],
    color_mixing: band.mixing,
  });
  if (drawn.ok !== true) fail(`渐变笔(${band.mixing}) ⇒ ${JSON.stringify(drawn).slice(0, 160)}`);
}

const raw = await call("render_region", { region: { x: 0, y: 0, w: docW, h: docH }, raw: true, max_px: 4_000_000 });
if (raw.ok !== true || !raw.raw_url) fail(`render_region ⇒ ${JSON.stringify(raw).slice(0, 160)}`);
const rendered = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
const exported = await call("export_png", { path: "export_under_linear.png", deterministic: true });
if (exported.ok !== true) fail(`export_png ⇒ ${JSON.stringify(exported).slice(0, 160)}`);
const file = typeof exported.path === "string" ? exported.path : null;
if (!file || !existsSync(file)) {
  console.error(`  ⊘ 不可判：拿不到导出的 PNG（path=${JSON.stringify(file)}）`);
  process.exit(3);
}
const decoded = execFileSync("ffmpeg", ["-v", "error", "-i", file, "-f", "rawvideo", "-pix_fmt", "rgba", "-"], {
  timeout: 120_000, maxBuffer: 256 * 1024 * 1024,
});
const pixelAt = (buffer, x, y) => {
  const at = (y * docW + x) * 4;
  return [buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]];
};

const picked = [];
for (const band of bands) {
  // **中段**：渐变在那里才混出第三种颜色
  const x = Math.round((x0 + x1) / 2);
  const renderedPixel = pixelAt(rendered, x, band.y);
  if (distance(renderedPixel, BACKGROUND) < 8) fail(`「${band.mixing}」重叠处仍是背景色 ⇒ 两笔没落上`);
  picked.push({ ...band, x, renderedPixel, exportedPixel: pixelAt(decoded, x, band.y) });
}

for (const hit of picked) {
  console.log(
    `  ${hit.mixing.padEnd(6)} (${hit.x},${hit.y}) 渲染 ${JSON.stringify(hit.renderedPixel.slice(0, 3))}｜导出 ${JSON.stringify(hit.exportedPixel.slice(0, 3))}`,
  );
  const delta = distance(hit.renderedPixel, hit.exportedPixel);
  if (delta !== 0) fail(`「${hit.mixing}」的导出像素与渲染像素不同（差 ${delta}）⇒ 导出路径又转了一次颜色`);
}
const delta = distance(picked[0].renderedPixel, picked[1].renderedPixel);
if (delta < 8) fail(`rgb 与 linear 的混色结果几乎相同（差 ${delta} 级）⇒ 混色模式没生效（或 linear 被实现成 rgb）`);
console.log(`✓ 混色下的导出：两条带子导出 == 渲染，且 linear 与 rgb 差 ${delta} 级`);
