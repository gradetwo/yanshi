#!/usr/bin/env node
// **笔速判据** ✓（第 1000+ 轮）：同一笔画，**慢笔必须比快笔落墨更多**。
//
// **为什么是这条** ✗：`dabs_per_second`（时间基间距）是"像真人"的核心 —— 真人**慢笔多墨、快笔飞白** ✓。
// 引擎（`hokusai`）实现了它 ✓，MCP 的 `points[i][3]` 也确实是时间 ✓ —— 但**单位是毫秒时间戳**✗，
// 不是"每段秒数"✗（`parse_brush_point_times` 里是 `(current - previous) / 1000.0` ✓）。
//
// **三个测量口径的坑（都是实测踩出来的 ✗）**：
//   ① 用 `painted_pixels`（**去重覆盖数**）当 dab 数 ✗ —— 重复落墨**不增加覆盖数**，测不出 ✓；
//   ② 用**不透明**色（α=255）✗ —— 同一区域重复落墨，颜色**几乎不变**，亮度测不出 ✓；
//   ③ 用"每段秒数"当时间 ✗ —— 差值恒为 0，时间等于没给 ✓。
// **∴ 正确做法 ✓：α=40（半透明，重复落墨会叠加变深）＋ 毫秒时间戳 ＋ 比 `avg_brightness`** ✓。
//
// 用法：node scripts/tool-brush-speed.mjs <server-base> <doc-id> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-speed.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return { status: reply.status, body: await reply.json() };
};
const BRUSH = "Clouds";
const XS = [60, 140, 220, 300, 340];
const points = (stepMs) =>
  XS.map((x, i) => [x, 100, 1, i * stepMs]);
const draw = async (layer, stepMs) => {
  await call("create_layer", { layer_id: layer, name: layer });
  const written = await call("brush_stroke", {
    layer_id: layer,
    brush: BRUSH,
    // **P-19 之后 `color.a` 会真乘进整笔 opacity** ⇒ 这里去掉顺带的 `a: 40` ✓：
    // 本判据测的是**时间间距 ⇒ 墨量**，与 alpha 无关；留着会把墨整体缩到 0.157 倍、
    // 亮度差被压小 ⇒ 与 P-19 无关的假红（判据自己造的输入，改判据不改产品）。
    color: { r: 20, g: 20, b: 20 },
    brush_params: { dabs_per_second: 200.0 },
    points: points(stepMs),
    preview: false,
  });
  if (written.status !== 200 || written.body?.ok !== true) {
    console.log(`  ✗ brush_stroke 失败（HTTP ${written.status}）：${JSON.stringify(written.body).slice(0, 200)}`);
    process.exit(1);
  }
  const measured = await call("analyze_region", { region: { x: 0, y: 0, w: 400, h: 200 } });
  await call("delete_layer", { layer_id: layer });
  const brightness = measured.body?.avg_brightness;
  if (typeof brightness !== "number") {
    console.log(`  ✗ analyze_region 没给 avg_brightness（${JSON.stringify(measured.body).slice(0, 160)}）`);
    process.exit(3);
  }
  return brightness;
};
const fast = await draw("speed_fast", 5);      // 每段 5ms
const slow = await draw("speed_slow", 500);    // 每段 500ms（慢 100 倍）
console.log(`  快笔（每段 5ms）  ⇒ avg_brightness = ${fast.toFixed(4)}`);
console.log(`  慢笔（每段 500ms）⇒ avg_brightness = ${slow.toFixed(4)}`);
// **判据 ✓**：慢笔的墨必须**明显**更多 ⇒ 亮度必须**显著更低** ✓（不设"小差异"门槛 ✗ —— 那是噪声问题 ✓）
const darker = fast - slow;
if (darker < 0.05) {
  console.log(`  ✗ 慢笔没有明显更暗（差 ${darker.toFixed(4)} < 0.05）⇒ 时间基间距没起作用`);
  process.exit(1);
}
console.log(`  ✓ 慢笔明显更暗（差 ${darker.toFixed(4)}）⇒ 笔速真的影响墨量`);
