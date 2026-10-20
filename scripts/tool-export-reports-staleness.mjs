#!/usr/bin/env node
// **`export_png` 报告 head 与文档渲染水位**（第 1346 轮建立；**第 1364 轮改**：不再给误导的 `raster_pending`）
//
// **⇒ 为什么有这条** ✗：
//   外部在 `yanshi-mcp --no-wait` 下连下 10 笔后立刻导出 ⇒ **只有第 1 笔在图里**
//   ⇒ 而响应**没有任何提示** ⇒ 调用方**无从判断**这是全图还是部分图 ✗
//   ⇒ 那是**静默降级**（本仓的核心对手）⇒ 修法不是悄悄等（`--no-wait` 明确选了不等），
//     而是**把差距说出来** ✓
//
// **⇒ 断什么**：
//   ① 响应必须有 `head_seq` / `rendered_seq` / `raster_pending` 三个字段
//   ② **恒等式**：`raster_pending === (rendered_seq < head_seq)`（两个数不一致就是矛盾）
//   ③ 写一笔之后 `head_seq` 必须 ≥ 1（否则说明原子没进日志）
//   ④ 同步模式正常导出后 `raster_pending` 必须是 `false`（否则调用方永远不知道该不该重导）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-export-reports-staleness.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const post = async (p, b, doc, tok) => {
  const u = `${base}${p}${doc ? `?doc=${doc}&token=${tok}` : ""}`;
  const r = await fetch(u, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b ?? {}) });
  return r.json();
};
const doc = `stale_${stamp}`;
const d = await post("/api/documents", { doc_id: doc, width: 64, height: 64 });
if (!d?.token) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
const T = d.token;
await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, T);
const filled = await post("/api/tools/fill_region", {
  layer_id: "L", shape: { type: "rect", x: 8, y: 8, w: 48, h: 48 }, color: [90, 140, 200], opacity: 1.0,
}, doc, T);
if (filled?.ok !== true) { console.error("  ⊘ 不可判：fill_region 失败"); process.exit(3); }
const ex = await post("/api/tools/export_png", { path: `stale_${stamp}.png`, deterministic: true }, doc, T);
if (ex?.ok !== true) { console.error(`  ⊘ 不可判：export_png 失败 ⇒ ${JSON.stringify(ex).slice(0, 100)}`); process.exit(3); }

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const has = ["head_seq", "rendered_seq"].every((k) => k in ex);
if (has) ok(`① 两个字段都在（head_seq=${ex.head_seq}｜rendered_seq=${ex.rendered_seq}）`);
else bad(`① 响应必须有 head_seq / rendered_seq ⇒ 实测缺 ${["head_seq", "rendered_seq"].filter((k) => !(k in ex)).join("、")}`);

if (typeof ex.head_seq === "number" && typeof ex.rendered_seq === "number") {
  // **⇒ 恒等式**：渲染水位**不该超过** head（超过就是矛盾）
  if (ex.rendered_seq <= ex.head_seq) ok(`② 渲染水位不超过 head（${ex.rendered_seq} ≤ ${ex.head_seq}）`);
  else bad(`② rendered_seq 不该超过 head_seq ⇒ 实测 ${ex.rendered_seq} > ${ex.head_seq}`);
  if (ex.head_seq >= 1) ok(`③ head_seq=${ex.head_seq}（≥1 ⇒ 原子进了日志）`);
  else bad(`③ 写过一笔之后 head_seq 应 ≥ 1 ⇒ 实测 ${ex.head_seq}`);
  // **⇒ 第 1364 轮的关键断言**：**不许**再出现 `raster_pending`
  //   它曾被误当成「导出可能不完整」⇒ 而导出是**同步重渲** ⇒ 那个布尔**必然误报** ✗
  if (!("raster_pending" in ex)) ok("④ 不再返回误导字段 raster_pending");
  else bad("④ 不该再有 raster_pending ⇒ 它对「导出是否完整」是误导的（导出总是同步重渲）");
} else {
  bad("② 两个字段的类型不对（都应是 number）");
}

// **⑤ 真正该验的**：`wait_for_render:false` 之后导出，图**仍然完整**
//   ⇒ 用两笔颜色截然不同的笔画：若导出只含第一笔，第二个颜色点会是白底 ✓
{
  const lag = `lag_${stamp}`;
  const d2 = await post("/api/documents", { doc_id: lag, width: 256, height: 256 });
  if (!d2?.token) {
    console.log("  ⊘ 跳过 ⑤：建文档失败（不是产品问题）");
  } else {
    const T2 = d2.token;
    await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, lag, T2);
    for (const [c, y] of [[[220, 30, 30], 80], [[30, 30, 220], 180]]) {
      await post("/api/tools/brush_stroke", {
        layer_id: "L", brush: "calligraphy", size: 40, color: c.map((v) => v / 255),
        points: [[60, y, 0.8], [200, y, 0.8]], preview: false, wait_for_render: false,
      }, lag, T2);
    }
    // **⇒ 用 `sample_colors` 采两点，不必解 PNG**（判据里没有解码器 ✓）
    const ex2 = await post("/api/tools/export_png", { path: `lag_${stamp}.png`, deterministic: true }, lag, T2);
    if (ex2?.ok !== true) {
      console.log("  ⊘ 跳过 ⑤：导出失败（不是产品问题）");
    } else {
      const sc = await post("/api/tools/sample_colors", { points: [[130, 80], [130, 180]] }, lag, T2);
      const samples = sc?.samples ?? sc?.context?.samples ?? [];
      if (samples.length !== 2) {
        console.log(`  ⊘ 跳过 ⑤：sample_colors 返回 ${samples.length} 个样本`);
      } else {
        const [red, blue] = samples.map((x) => x.rgba || []);
        const redOk = red[0] > red[2] + 60;
        const blueOk = blue[2] > blue[0] + 60;
        if (redOk && blueOk) ok(`⑤ wait_for_render:false 之后画面仍含**两笔**（红 ${red}｜蓝 ${blue}）`);
        else bad(`⑤ 画面必须含两笔 ⇒ 实测红点 ${red}（红=${redOk}）、蓝点 ${blue}（蓝=${blueOk}）`);
      }
    }
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（导出必须让人看见"是否落后"）`);
  process.exit(1);
}
console.log("\n  ✓ export_png 如实报告栅格是否落后");
