#!/usr/bin/env node
// **预览渲染计数判据**（第 274 轮 ✓）：**"空文档的冷启动预览不必光栅化"这条优化要能被守住** ✗ → ✓。
//
// **为什么需要它** ✗：
//   第 273 轮把"**空白文档 ＋ 不透明背景**"的冷启动预览改成**不做整幅光栅化** ✓
//   （改为构造一份均匀缓冲，喂给**同一条**块平均路径 ✓ ⇒ 缩略图**逐字节相同** ✓，
//    实测 4K 首次 `get_document`：**808 ms → 141 ms** ✓）。
//   但当时**没有能红的判据** ✗：
//     * `tool-timing-attribution.mjs` 判的是**归属**（`preview_ms > 0` ✓、残差占比 ✓）
//       ⇒ 对"跳过了光栅化"**不会变红** ✗（归属仍然正确 ✓）；
//     * `tool-thumbnail-cold.mjs` 用 **320×240** 文档 ✓、预算是**按渲染成本标定**的模型 ✓
//       ⇒ 在新路径上**过宽** ✗；
//     * 而真正该用的那两个计数（"这条路真的走了" ✓）**是私有字段、不进任何响应** ✗。
//   ⇒ 第 274 轮给它们加了只读出口 ✓（`get_document` 响应里的 `preview_renders` /
//     `full_canvas_renders` ✓）⇒ **本判据读它们** ✓。
//
// **判据（两条，各自能红 ✓，而且是**结构性**的 ⇒ 与区域大小、机器快慢、墙钟都无关 ✓）**：
//   ① **空文档**（没有一个对象 ✓）的**第一次** `get_document` ⇒ `preview_renders` **必须为 0** ✓
//      —— 即**没有**走光栅化 ✓。
//      变异检验 ✓：把第 273 轮的跳过改回去 ⇒ 这里变成 1 ⇒ **本判据变红** ✓。
//   ② **有内容的文档**（画一笔之后 ✓）⇒ `preview_renders` **必须 ≥ 1** ✓
//      —— **这条是防作弊** ✓：否则"干脆永远不渲染"也能让 ① 通过 ✗，
//      而那样用户看到的就是一张**永远不更新**的缩略图 ✓（正是本判据要防的失败模式 ✓）。
//
// **不写死观测值** ✓：两条都拿**当次运行**的计数与"0 / ≥1"这个**语义边界**比 ✓；
//   唯一固定的是"空文档"与"有内容"这两个**场景**（那是判据的定义 ✓，不是某次观测 ✓）。
//   **不看时间** ✗ ⇒ 不会在慢机器上假红 ✓。
//
// 用法：node scripts/tool-preview-render-count.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-preview-render-count.mjs <base-url>");
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
const doc = "preview-count-" + stamp;

const created = await post("/api/documents", { doc_id: doc, width: 3840, height: 2160 });
if (!created.token) {
  console.error("✗ 前置不成立：建不出 4K 文档 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const token = created.token;
const getDocument = () =>
  post("/api/tools/get_document?doc=" + encodeURIComponent(doc) + "&token=" + token, {});

// —— ① 空文档：第一次取状态 ——
const first = await getDocument();
if (!first.ok) {
  console.error("✗ 前置不成立：get_document 失败 ⇒ " + JSON.stringify(first).slice(0, 160));
  process.exit(2);
}
if (first.preview_renders === undefined || first.full_canvas_renders === undefined) {
  console.error(
    "✗ 前置不成立：响应里没有 `preview_renders` / `full_canvas_renders` " +
    "⇒ 判据无法运行（不是通过）⇒ " + JSON.stringify(Object.keys(first)).slice(0, 200),
  );
  process.exit(2);
}
console.log(
  `  空文档（4K）第一次 get_document：preview_renders=${first.preview_renders}` +
  ` full_canvas_renders=${first.full_canvas_renders}`,
);
check(
  first.preview_renders === 0,
  "空文档的冷启动预览不做光栅化（preview_renders == 0）",
  "preview_renders=" + first.preview_renders,
);

// —— ② 有内容的文档：画一笔之后必须真的渲染过 ——
const stroke = await post(
  "/api/tools/brush_stroke?doc=" + encodeURIComponent(doc) + "&token=" + token,
  {
    layer_id: "layer_default",
    brush: "100%_Opaque",
    points: [[100, 100, 1], [400, 100, 1]],
    size: 40,
    color: { r: 0, g: 0, b: 0, a: 255 },
  },
);
if (stroke.ok !== true) {
  console.error(
    "✗ 前置不成立：画不出这一笔 ⇒ 判据无法运行（不是通过）⇒ " +
    JSON.stringify(stroke).slice(0, 200),
  );
  process.exit(2);
}
const after = await getDocument();
console.log(
  `  画一笔之后：preview_renders=${after.preview_renders}` +
  ` full_canvas_renders=${after.full_canvas_renders}`,
);
check(
  after.preview_renders >= 1,
  "有内容的文档确实走了渲染（preview_renders ≥ 1）—— 这条防「永远不渲染」",
  "preview_renders=" + after.preview_renders,
);

console.log("");
if (failures.length) {
  console.error(`结论：预览渲染计数不合格 ✗（${failures.length} 条）`);
  for (const item of failures) console.error("   - " + item);
  process.exit(1);
}
console.log("结论：✓ 空文档不渲染、有内容必渲染（结构性判据，与时间无关）");
process.exit(0);
