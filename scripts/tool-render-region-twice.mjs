#!/usr/bin/env node
// **连续两次 `render_region(raw)` 必须给出同一幅画面** ✓（第 650 轮；第 649 轮实测到的可疑现象）
//
// **⇒ 症状（第 649 轮）**：同一文档、同一点 (32,16)：
//   `sample_colors` ⇒ `[249,137,124]` ✓、**第一次** `render_region` ⇒ `[249,137,124]` ✓、
//   **第二次** `render_region` ⇒ 整幅**只剩背景** ✗。
//
// **⇒ 判据**：填一块纯色 ⇒ **同一区域连渲染两次** ⇒
//   ① 两次的（中心）像素**必须相同** ✓；
//   ② 且都**必须等于所填颜色** ✓（否则"两次相同但都错"也能过 ✗）。
//
// **变异**：让第二次渲染复用一份失效的缓存（或让 raw 路径返回旧 CAS 条目）⇒ ① 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-render-region-twice.mjs <base> <doc> <token>");
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
const WANT = { r: 210, g: 60, b: 40 };
const filled = await call("fill_region", {
  layer_id: "layer_default",
  shape: { type: "rect", x: 0, y: 0, w: docW, h: docH },
  color: { ...WANT, a: 255 },
  opacity: 1.0,
});
if (filled.ok !== true) fail(`fill_region ⇒ ${JSON.stringify(filled).slice(0, 140)}`);

// **再画一笔** ✓（第 650 轮：只 `fill_region` 时两次渲染一致；有笔触才复现"第二次丢内容"）
const strokeY = Math.floor(docH * 0.3);
const stroke = await call("brush_stroke", {
  layer_id: "layer_default",
  brush: "calligraphy",
  points: [
    [Math.round(docW * 0.2), strokeY, 0.5],
    [Math.round(docW * 0.8), strokeY, 0.5],
  ],
  size: Math.max(6, Math.round(docH * 0.12)),
  color: [0.95, 0.25, 0.2, 1.0],
});
if (stroke.ok !== true) fail(`brush_stroke ⇒ ${JSON.stringify(stroke).slice(0, 160)}`);

// **再加一笔：半透明 + `color_mixing: "linear"`** ✓（复现第 649 轮的确切序列）
const stroke2Y = Math.floor(docH * 0.7);
const stroke2 = await call("brush_stroke", {
  layer_id: "layer_default",
  brush: "calligraphy",
  points: [
    [Math.round(docW * 0.2), stroke2Y, 0.5],
    [Math.round(docW * 0.8), stroke2Y, 0.5],
  ],
  size: Math.max(6, Math.round(docH * 0.12)),
  color: [0.3, 0.9, 0.4, 0.5],
  color_mixing: "linear",
});
if (stroke2.ok !== true) fail(`brush_stroke(linear, 半透明) ⇒ ${JSON.stringify(stroke2).slice(0, 160)}`);

const centre = (buffer) => {
  const at = (strokeY * docW + Math.round(docW * 0.5)) * 4;
  return [buffer[at], buffer[at + 1], buffer[at + 2], buffer[at + 3]];
};
const renderOnce = async (label) => {
  const raw = await call("render_region", { region: { x: 0, y: 0, w: docW, h: docH }, raw: true, max_px: 4_000_000 });
  if (raw.ok !== true || !raw.raw_url) fail(`${label} render_region ⇒ ${JSON.stringify(raw).slice(0, 160)}`);
  const buffer = Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
  if (buffer.length < docW * docH * 4) fail(`${label} 的 raw 太小：${buffer.length} 字节`);
  return centre(buffer);
};

const first = await renderOnce("第一次");
const second = await renderOnce("第二次");
console.log(`  第一次 ${JSON.stringify(first)}｜第二次 ${JSON.stringify(second)}`);
const same = first.every((value, index) => value === second[index]);
if (!same) {
  fail(`两次渲染**不一致** ⇒ 第二次丢了图层内容（第 649 轮实测：第二次整幅只剩背景）`);
}
// **笔迹处必须既不同于背景、又两次一致** ✓
const isBackground = first[0] === WANT.r && first[1] === WANT.g && first[2] === WANT.b;
if (isBackground) {
  fail(`笔迹处仍是背景色 ⇒ 这一笔没进渲染（或两次渲染都丢了它）`);
}
console.log("✓ 连渲染两次：像素一致，且都等于所填颜色");
