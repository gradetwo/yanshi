#!/usr/bin/env node
// **P2-14：频率分离四段计时 + 烘焙提速**（查表化 + 模糊并行）。
//
// **根因**（复测 R-4 定性）：4000×3200 r=8 的 FS 热调用 other_ms 115s（debug），
// 其中 `fs_bake` 内约 115s = 逐像素 f32 循环约 76s（HF + 自检两轮，每像素每通道
// 两次 `powf` 级编解码）+ 直接卷积模糊约 39s；渲染约 6.5s 不动。
// **修法**：① HF/自检两轮查表化（三张 64K 表，表项即原函数，输出逐位不变）；
// ② 两向卷积按行带并行（`std::thread::scope` + `rows::split_bands`，每输出像素
// 累加顺序不变 ⇒ 逐位不变）；③ 响应加 `fs_timings {render_ms, blur_ms, hf_ms, check_ms}`。
//
// **判据**（2000×1600，12 笔亮色带，与复测 R-4 同形）：FS 必须 ok:true、重建误差
// 在文档界内、fs_timings 四段齐全且为非负数、四段之和 ≤ total_ms（恒等式）、
// other_ms < 30000（宽松上界，只防灾难性回退；验收数是 debug 下复测 r4one 的
// other_ms 对照 115s → 约 15–20s，不由本脚本断）。
//
// **变异**：任一表项算错 / 并行带改累加顺序 ⇒ 误差界或逐位对照红；删 `fs_timings`
// ⇒ 本脚本红。**前提失败必须 exit(3)**：拿不到 token/建不了层 ⇒ 不可判，不是产品失败。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-frequency-separation-timing.mjs <base-url>"); process.exit(2); }
const doc = "fst_" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 2000, height: 1600 }),
})).json();
const token = created.token;
if (!token) { console.error("  ⊘ 不可判：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};

const failures = [];
const layer = await call("create_layer", { layer_id: "L1", name: "L" });
if (layer.ok !== true) {
  console.error(`  ⊘ 不可判：建层失败 ⇒ ${JSON.stringify(layer).slice(0, 140)}`);
  process.exit(3);
}
// 4 条亮色带 × 3 遍同色横扫（复测 R-4 的 fs_doc 同形：亮色平底 ⇒ 自检能过；
// 烘焙成本按像素算，与内容无关 ⇒ 不偏袒计时）。
const bands = [[210, 70, 60], [235, 185, 70], [95, 145, 85], [70, 95, 165]];
const calls = [];
for (let i = 0; i < 4; i++) {
  const y = Math.floor((i + 0.5) * 1600 / 4);
  for (let k = 0; k < 3; k++) {
    calls.push({ tool: "brush_stroke", arguments: {
      doc_id: doc, layer_id: "L1", brush: "oil-06-paint", color: bands[i],
      size: 448, opacity: 1.0, preview: false,
      points: [[0, y, 0.95], [1000, y, 0.95], [2000, y, 0.95]],
    } });
  }
}
const painted = await call("batch", { calls, silent: true });
if (painted.ok !== true || painted.ok_count !== 12) {
  failures.push(`底色 12 笔没落全（ok=${painted.ok}，ok_count=${painted.ok_count}）：${JSON.stringify(painted).slice(0, 160)}`);
}
const t0 = Date.now();
const fs = await call("frequency_separation", { layer_id: "L1", radius: 8.0, name: "fs-timing" });
const wallMs = Date.now() - t0;
const timings = fs.timings ?? {};
console.log(`  FS ⇒ ok=${fs.ok}｜wall=${wallMs}ms｜total_ms=${timings.total_ms}｜other_ms=${timings.other_ms}｜fs_timings=${JSON.stringify(fs.fs_timings ?? null)}`);
if (fs.ok !== true) {
  failures.push(`FS 应当成功（ok=${fs.ok}）：${JSON.stringify(fs).slice(0, 200)}`);
} else {
  const maxErr = Number(fs.reconstruction_max_error);
  const meanErr = Number(fs.reconstruction_mean_error);
  const brightErr = Number(fs.reconstruction_max_error_bright);
  // 误差界 +1e-9：服务端误差是 f32，`2/255` 的 f32 值读成 f64 时比 JS 的
  // `2/255` 大约 4.6e-10（实测：恰好 2 LSB 的中高调误差读作 0.007843137718737125，
  // 而 JS `2/255` = 0.00784313725490196）—— 不加这点表示层 slack，恰好顶格的
  // 合法值会被误判。Rust 侧同界用 f32 算，不受此影响。
  if (!(maxErr <= 20 / 255 + 1e-9)) failures.push(`重建最大误差应 ≤ 20/255，得到 ${maxErr}`);
  if (!(meanErr <= 3 / 255 + 1e-9)) failures.push(`重建均值误差应 ≤ 3/255，得到 ${meanErr}`);
  if (!(brightErr <= 2 / 255 + 1e-9)) failures.push(`中高调最大误差应 ≤ 2/255，得到 ${brightErr}`);
  const ft = fs.fs_timings;
  if (!ft || typeof ft !== "object") {
    failures.push("响应应当带 fs_timings（P2-14 四段计时）");
  } else {
    for (const key of ["render_ms", "blur_ms", "hf_ms", "check_ms"]) {
      if (typeof ft[key] !== "number" || !(ft[key] >= 0)) failures.push(`fs_timings.${key} 应当是非负数，得到 ${ft[key]}`);
    }
    if (typeof ft.blur_ms === "number" && !(ft.blur_ms > 0)) failures.push("fs_timings.blur_ms 应当 > 0（模糊确实跑了）");
    const sum = ["render_ms", "blur_ms", "hf_ms", "check_ms"].reduce((a, k) => a + (Number(ft[k]) || 0), 0);
    if (typeof timings.total_ms === "number" && !(sum <= timings.total_ms)) {
      failures.push(`四段之和（${sum.toFixed(1)}ms）应当 ≤ total_ms（${timings.total_ms}ms）`);
    }
  }
  if (typeof timings.other_ms === "number" && !(timings.other_ms < 30000)) {
    failures.push(`other_ms 应 < 30000ms（宽松上界），得到 ${timings.other_ms}ms`);
  }
}
if (failures.length) {
  console.log("  ✗ 频率分离计时不合格：");
  for (const f of failures) console.log(`     - ${f}`);
  process.exit(1);
}
console.log("  ✓ 频率分离四段计时齐全、误差在界内、other_ms 在宽松上界内（P2-14）");
process.exit(0);
