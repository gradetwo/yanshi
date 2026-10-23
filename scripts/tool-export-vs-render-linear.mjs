#!/usr/bin/env node
// **线性混色的笔触：导出像素必须等于渲染像素，且 linear 必须与 rgb 不同** ✓（第 651 轮；色彩 Phase 1）
//
// **⇒ 为什么单列**：`tool-export-matches-render.mjs` 只查 `fill_region` 的**纯色** ⇒ 它抓不到
//   "混色算在 linear 里、导出又要转一次"这类错误 ✗。
//
// **⇒ 判据（三条）**：
//   ① 两条带子：`render_region` 的像素 **精确等于** `export_png` 解码出来的像素 ✓（PNG 无损）；
//   ② 两条带子都**真的画上**（与背景差 ≥ 8）✓；
//   ③ `linear` 与 `rgb` 两条带子的像素**必须不同**（差 ≥ 8）✓ —— 否则混色模式没生效 ✓。
//
// **变异**：把 `ColorMixing::Linear` 实现成与 `Rgb` 相同 ⇒ ③ 红 ✓。
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-export-vs-render-linear.mjs <base> <doc> <token>");
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
const BACKGROUND = { r: 40, g: 60, b: 90 };
const distance = (a, b) => Math.abs(a[0] - b[0]) + Math.abs(a[1] - b[1]) + Math.abs(a[2] - b[2]);
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: docW, h: docH },
  color: { ...BACKGROUND, a: 255 },
  opacity: 1.0,
});
if (filled.ok !== true) fail(`fill_region ⇒ ${JSON.stringify(filled).slice(0, 140)}`);

// 两条横带：上面 `rgb`、下面 `linear`（同色同透明度）
const bandH = Math.floor(docH / 2);
const bands = [
  { mixing: "rgb", y: Math.floor(bandH * 0.5) },
  { mixing: "linear", y: bandH + Math.floor(bandH * 0.5) },
];
for (const band of bands) {
  const painted = await call("brush_stroke", {
    layer_id: "layer_default",
    brush: "calligraphy",
    points: [
      [Math.round(docW * 0.2), band.y, 0.5],
      [Math.round(docW * 0.8), band.y, 0.5],
    ],
    size: Math.max(6, Math.round(docH * 0.12)),
    color: [0.95, 0.25, 0.2, 1.0],
    color_mixing: band.mixing,
  });
  if (painted.ok !== true) fail(`brush_stroke(${band.mixing}) ⇒ ${JSON.stringify(painted).slice(0, 160)}`);
  console.log(`  [probe] ${band.mixing}: painted_pixels=${painted.painted_pixels}｜warnings=${JSON.stringify(painted.warnings ?? "").slice(0, 120)}`);
}

// **渲染两次** ✓：第 649/651 轮都看到"第一次拿到的是旧画面"
const renderFull = async (label) => {
  const answer = await call("render_region", { region: { x: 0, y: 0, w: docW, h: docH }, raw: true, max_px: 4_000_000 });
  if (answer.ok !== true || !answer.raw_url) fail(`${label} render_region ⇒ ${JSON.stringify(answer).slice(0, 160)}`);
  return Buffer.from(await (await fetch(base + answer.raw_url)).arrayBuffer());
};
const renderedFirst = await renderFull("第一次");
const rendered = await renderFull("第二次");
const countPainted = (buffer) => {
  let count = 0;
  for (let at = 0; at + 3 < buffer.length; at += 4) {
    if (distance([buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]], BACKGROUND) >= 8) count += 1;
  }
  return count;
};
console.log(`  [probe] 第一次渲染画到的像素=${countPainted(renderedFirst)}｜第二次=${countPainted(rendered)}`);
console.log(`  [probe] raw 响应键=${JSON.stringify(Object.keys(raw))}｜bytes=${rendered.length}｜docW×docH×4=${docW * docH * 4}`);
console.log(`  [probe] raw 尺寸字段：width=${raw.width}｜height=${raw.height}｜region=${JSON.stringify(raw.region ?? null)}`);

const exported = await call("export_png", { path: "export_vs_render_linear.png", deterministic: true });
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
  let hit = null;
  for (let y = Math.max(0, band.y - 4); y <= Math.min(docH - 1, band.y + 4) && !hit; y += 1) {
    for (let x = 0; x < docW; x += 1) {
      const renderedPixel = pixelAt(rendered, x, y);
      if (distance(renderedPixel, BACKGROUND) >= 8) {
        hit = { ...band, x, y, renderedPixel, exportedPixel: pixelAt(decoded, x, y) };
        break;
      }
    }
  }
  if (!hit) fail(`「${band.mixing}」带子里没有画到的像素 ⇒ 笔触没落上`);
  picked.push(hit);
}

for (const hit of picked) {
  console.log(
    `  ${hit.mixing.padEnd(6)} (${hit.x},${hit.y}) 渲染 ${JSON.stringify(hit.renderedPixel.slice(0, 3))}｜导出 ${JSON.stringify(hit.exportedPixel.slice(0, 3))}`,
  );
  const delta = distance(hit.renderedPixel, hit.exportedPixel);
  if (delta !== 0) {
    fail(`「${hit.mixing}」的导出像素与渲染像素不同（差 ${delta}）⇒ 导出路径又转了一次颜色`);
  }
}
const delta = distance(picked[0].renderedPixel, picked[1].renderedPixel);
if (delta < 8) {
  fail(`rgb 与 linear 两条带子几乎相同（差 ${delta} 级）⇒ 混色模式没生效（或 linear 被实现成 rgb）`);
}
console.log(`✓ 线性混色：导出 == 渲染（两条带子精确相等），且 linear 与 rgb 差 ${delta} 级`);
