#!/usr/bin/env node
// **计时归属判据**（第 271 轮 ✓）：**响应里的分段计时必须覆盖住真实成本** ✗ → ✓。
//
// **为什么需要它** ✗（本会话实测 ✓）：
//   `get_document` 在**读路径**上会生成文档预览（`ensure_document_thumbnail` ⇒
//   `render_document_preview` 或 `thumbnail` ✓）。而 `timings.rs` 的 `Phase::Preview`
//   此前**只在写入路径**（`finish_mutation` ⇒ `run_pending_jobs`）被填 ✗
//   ⇒ **∴ 读路径那一段一个阶段都不记** ✗ ⇒ **∴ 它整段落进残差 `other_ms`** ✓。
//   **实测**（release，全新 4K 文档，第一次 `get_document` ✓）：
//     `preview_ms=0.0` ✗｜`render_ms=0.0`｜`raster_ms=0.0`｜`png_ms=0.0`
//     ｜**`other_ms=859.995`** ✗｜`total_ms=860.0` ✓
//   ⇒ 报告方**只能从外部反推** ✓ —— 而 `timings.rs` 自己的文档就警告过这个失败模式
//     （"渲染与编码全被残差吞掉" ✓）。**∴ 没有判据守它** ✗ ⇒ 才能藏这么久 ✓。
//
// **判据**（两条，各自能红 ✓）：
//   ① **读路径的预览渲染必须是**非零**阶段** ✓：全新 4K 文档第一次 `get_document` ⇒
//      `preview_ms > 0` ✓（**变异检验**：删掉 `read_get_document` 里那处 `ctx.time(Phase::Preview, …)`
//      ⇒ `preview_ms` 回 0 ⇒ **本判据变红** ✓）。
//   ② **残差不得吞掉成本** ✓：当 `total_ms ≥ 50 ms` 时，要求 `other_ms ≤ 10% × total_ms` ✓
//      （实测修好后是 **0.02 / 842.9 ≈ 0.002%** ✓；修之前是 **100%** ✗ ⇒ 余量极大 ✓）。
//      设 50 ms 的门槛是为了**只在真有成本时判** ✗ ⇒ 否则快速路径上 1 ms 的抖动会让比值乱跳 ✓。
//
// **为什么用 4K** ✗：成本随像素数增长（320×240 → 26 ms；3840×2160 → 942 ms ✓）
//   ⇒ 用大文档才能让"未测量的成本"大到看得出来 ✓。
//
// **不写死期望值** ✓：两条都拿**当次运行**的 `total_ms` / `other_ms` / `preview_ms` 互相比 ✓，
//   唯一固定的是"4K 文档"与"50 ms 门槛 / 10% 占比"这两个**判据参数**（写在注释里 ✓）。
//
// 用法：node scripts/tool-timing-attribution.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-timing-attribution.mjs <base-url>");
  process.exit(2);
}

const failures = [];
function check(ok, label, detail) {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
}

async function post(path, body) {
  const response = await fetch(base + path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body || {}),
  });
  return response.json();
}

const stamp = Date.now().toString(36);

// **先热身** ✓：进程内第一次渲染还要付一次性的初始化 ✓，那不是我们量的东西 ✗
// （实测：不热身时首次要 868 ms，其中大部分是初始化 ✓）。
{
  const warm = "timing-warm-" + stamp;
  const created = await post("/api/documents", { doc_id: warm, width: 64, height: 64 });
  if (!created.token) {
    console.error("✗ 前置不成立：拿不到 token ⇒ 判据无法运行（不是通过）");
    process.exit(2);
  }
  await post("/api/tools/get_document?doc=" + encodeURIComponent(warm) + "&token=" + created.token, {});
}

// **判据对象**：一个全新的 4K 文档 ⇒ 第一次取状态要生成文档预览 ✓。
const doc = "timing-attribution-" + stamp;
const created = await post("/api/documents", { doc_id: doc, width: 3840, height: 2160 });
if (!created.token) {
  console.error("✗ 前置不成立：4K 文档建不出来 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const started = Date.now();
const value = await post(
  "/api/tools/get_document?doc=" + encodeURIComponent(doc) + "&token=" + created.token,
  {},
);
const wall = Date.now() - started;
if (!value.ok) {
  console.error("✗ 前置不成立：get_document 失败 ⇒ " + JSON.stringify(value).slice(0, 160));
  process.exit(2);
}
const timings = value.timings || {};
const total = Number(timings.total_ms);
const other = Number(timings.other_ms);
const preview = Number(timings.preview_ms);
if (!Number.isFinite(total) || !Number.isFinite(other) || !Number.isFinite(preview)) {
  console.error("✗ 前置不成立：响应里没有分段计时 ⇒ " + JSON.stringify(timings).slice(0, 160));
  process.exit(2);
}

console.log(`  4K 首次 get_document：端到端 ${wall} ms｜total_ms ${total}｜preview_ms ${preview}｜other_ms ${other}`);
console.log(
  "  各相：" +
  Object.keys(timings).sort().map((k) => k.replace("_ms", "") + "=" + timings[k]).join(" "),
);

// ① 读路径的预览渲染必须是**非零**阶段 ✓。
check(preview > 0, "读路径的预览渲染被计时（preview_ms > 0）", "preview_ms=" + preview);

// ② 残差不得吞掉成本 ✓（仅在真有成本时判 ✓）。
if (total >= 50) {
  const share = (other / total) * 100;
  check(
    share <= 10,
    "残差没有吞掉成本（other_ms ≤ 10% × total_ms）",
    "other_ms/total_ms = " + share.toFixed(3) + "%",
  );
} else {
  console.log(
    `  ⊘ total_ms=${total} < 50 ms ⇒ 这条太快，占比判据不适用（不判 ⇒ 也不假装通过）`,
  );
  // **仍然要能红** ✗：若总时长很小而残差却是全部 ⇒ 说明连"快"都是假的 ✓。
  if (total > 0 && other >= total) {
    check(false, "总时长很小但残差就是全部 ⇒ 那段工作没被测量", "other=" + other + " total=" + total);
  }
}

console.log("");
if (failures.length) {
  console.error(`结论：计时归属不合格 ✗（${failures.length} 条）⇒ 有成本没被测量`);
  for (const item of failures) console.error("   - " + item);
  process.exit(1);
}
console.log("结论：✓ 读路径的预览渲染被计入 preview，且残差没有吞掉成本");
process.exit(0);
