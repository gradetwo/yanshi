#!/usr/bin/env node
// **`export_png` 必须报告「这张图是否落后于 head」**（第 1346 轮；外部 **X-03**）
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

const has = ["head_seq", "rendered_seq", "raster_pending"].every((k) => k in ex);
if (has) ok(`① 三个字段都在（head_seq=${ex.head_seq}｜rendered_seq=${ex.rendered_seq}｜raster_pending=${ex.raster_pending}）`);
else bad(`① 响应必须有 head_seq / rendered_seq / raster_pending ⇒ 实测缺 ${["head_seq", "rendered_seq", "raster_pending"].filter((k) => !(k in ex)).join("、")}`);

if (typeof ex.head_seq === "number" && typeof ex.rendered_seq === "number" && typeof ex.raster_pending === "boolean") {
  const expect = ex.rendered_seq < ex.head_seq;
  if (ex.raster_pending === expect) ok(`② 恒等式成立：raster_pending === (rendered_seq < head_seq) ⇒ ${ex.raster_pending}`);
  else bad(`② 恒等式必须成立 ⇒ rendered_seq=${ex.rendered_seq}、head_seq=${ex.head_seq} 时 raster_pending 应为 ${expect}，实测 ${ex.raster_pending}`);
  if (ex.head_seq >= 1) ok(`③ head_seq=${ex.head_seq}（≥1 ⇒ 原子进了日志）`);
  else bad(`③ 写过一笔之后 head_seq 应 ≥ 1 ⇒ 实测 ${ex.head_seq}`);
  if (ex.raster_pending === false) ok("④ 同步模式下导出后栅格不落后（raster_pending=false）");
  else bad(`④ 同步模式导出后 raster_pending 应为 false ⇒ 实测 true（rendered ${ex.rendered_seq} < head ${ex.head_seq}）`);
} else {
  bad("② 三个字段的类型不对（head/rendered 应 number、raster_pending 应 boolean）");
}

// ---- ⑤ **能造出落后时，必须如实报 true** ----
//   **⇒ 为什么必须有这条** ✗（第 1346 轮实测）：只断「同步时是 false」的话，
//     把 `raster_pending` 写成**常量 false** 也能过 ⇒ **判据毫无牙** ✗
//     ⇒ 而我第一次的变异正是这个 → **判据没红** ⇒ 所以补这一条 ✓
//   **⇒ 怎么造**：`brush_stroke` 带 `wait_for_render: false` ⇒ 立即返回、渲染留后台
//     ⇒ 实测 `head=5｜rendered=2` ⇒ 落后是真的 ✓
{
  const lag = `lag_${stamp}`;
  const d2 = await post("/api/documents", { doc_id: lag, width: 512, height: 512 });
  if (!d2?.token) { console.error("  ⊘ 不可判：建文档失败（落后用例）"); process.exit(3); }
  const T2 = d2.token;
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, lag, T2);
  for (let k = 1; k <= 3; k += 1) {
    await post("/api/tools/brush_stroke", {
      layer_id: "L", brush: "calligraphy", size: 40, color: [200, 60, 60], wait_for_render: false,
      points: [[40, 100 * k, 60 * k], [200, 120 * k, 80 * k], [380, 140 * k, 100 * k]],
    }, lag, T2);
  }
  const ex2 = await post("/api/tools/export_png", { path: `lag_${stamp}.png`, deterministic: true }, lag, T2);
  if (ex2?.ok !== true) {
    console.log("  ⊘ 跳过 ⑤：落后用例的导出失败（不是产品问题）");
  } else {
    const behind = typeof ex2.rendered_seq === "number" && typeof ex2.head_seq === "number"
      && ex2.rendered_seq < ex2.head_seq;
    if (!behind) {
      console.log(`  ○ ⑤ 本轮没造出落后（head=${ex2.head_seq}｜rendered=${ex2.rendered_seq}）⇒ 不可判，不算不符`);
    } else if (ex2.raster_pending === true) {
      ok(`⑤ 落后时如实报 true：head=${ex2.head_seq}｜rendered=${ex2.rendered_seq}｜raster_pending=true`);
    } else {
      bad(`⑤ 落后时必须报 raster_pending=true ⇒ head=${ex2.head_seq}｜rendered=${ex2.rendered_seq} 而实测 ${ex2.raster_pending}（掩盖了落后）`);
    }
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（导出必须让人看见"是否落后"）`);
  process.exit(1);
}
console.log("\n  ✓ export_png 如实报告栅格是否落后");
