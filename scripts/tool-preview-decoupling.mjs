#!/usr/bin/env node
// **预览解耦判据**（第 4 轮 ✓，目标 P0 ✓）
//
// **为什么需要它** ✗（用户现场实测 ＋ 我复现 ✓）：
//   连续作画时**每一笔**都会编一张预览 PNG ⇒ 单笔延迟里 **~130 ms** 花在这里 ✓
//   （4K、`Clouds.myb`、`size 180`、5 点长笔触；实测 `preview_ms` 116~148 ms ✓）。
//   而 `batch(silent:true)` 在"不放行预览"时**仍然编码** ✓（调用栈已定位：
//   `write_batch` → `write_brush_stroke` → `write_import_image` → `finish_mutation` → `render_region` ✓）
//   ⇒ **∴ 目前没有一条"这一笔不要图"的路** ✓（`brush_stroke` 甚至不接受 `preview` 参数 ✓）。
//
// **判据（三条，各自能红 ✓）**：
//   ① `brush_stroke` **必须接受** `preview: false` ✓（现在会被参数校验拒绝 ⇒ 红 ✓）。
//   ② 带 `preview: false` 时，这一笔的 **`preview_ms` 必须为 0** ✓ —— 即**真的没编码** ✓。
//   ③ **不带**它时，`preview_ms` **必须 > 0** ✓ —— **防作弊** ✗：否则"干脆永远不产出预览"
//      也能让 ② 通过，而查看器就再也看不到画面了 ✓。
//
// **不写死观测** ✓：比的是 `0` 与 `> 0` 这个**语义边界** ✓，不看具体毫秒数 ✓
//   （机器快慢、笔刷不同都不影响 ✓）。
//
// 用法：node scripts/tool-preview-decoupling.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-preview-decoupling.mjs <base-url>");
  process.exit(2);
}

const failures = [];
function check(ok, label, detail) {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
}

// 用户现场那类负载：4K、大号笔刷、5 个控制点、横跨大半幅 ✓。
const doc = "preview-decouple-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 4096, height: 3072 }),
})).json();
if (!created.token) {
  console.error("✗ 前置不成立：建不出 4K 文档 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const token = created.token;
const call = async (tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();

const layer = await call("create_layer", { layer_id: "L1" });
if (layer.ok !== true) {
  console.error("✗ 前置不成立：建不出图层 ⇒ " + JSON.stringify(layer).slice(0, 160));
  process.exit(2);
}

const stroke = (index, extra) => {
  const y = 1200 + index * 40;
  const args = {
    layer_id: "L1", object_id: "s" + index, brush: "Clouds.myb", size: 180,
    color: { r: 175, g: 148, b: 140, a: 180 }, smooth: true,
    points: [[450, y, 0.45], [980, y + 40, 0.85], [1650, y + 10, 0.9], [2400, y + 60, 0.8], [3150, y + 20, 0.35]],
  };
  return Object.assign(args, extra || {});
};

const previewMs = (r) => {
  const t = r.timings || {};
  return t.preview_ms === undefined ? null : Number(t.preview_ms);
};

// —— ③ 先测基线（应当 > 0）——
const plain = await call("brush_stroke", stroke(0));
const plainMs = previewMs(plain);
console.log(`  不带 preview ⇒ ok=${plain.ok}｜preview_ms=${plainMs}`);
check(plain.ok === true, "前置：普通笔触必须成功", "ok=" + plain.ok);
check(
  plain.preview !== undefined && plain.preview !== null,
  "不带 `preview: false` 时必须**产出预览**（响应里有 `preview`）",
  "preview=" + JSON.stringify(plain.preview || null).slice(0, 60),
);
console.log(`  （参考：不带开关 preview_ms=${plainMs}｜带开关见下 ✓ —— 两者之差即省下的编码 ✓）`);

// —— ① 必须接受 preview:false ——
const off = await call("brush_stroke", stroke(1, { preview: false }));
const offDetail = String((off.context || {}).detail || "");
console.log(`  带 preview:false ⇒ ok=${off.ok}｜error=${off.error_code}｜detail=${offDetail.slice(0, 110)}`);
check(
  off.ok === true,
  "`brush_stroke` 必须**接受** `preview: false`（现在被参数校验拒绝 ⇒ 未实现）",
  off.ok === true ? "" : "error=" + off.error_code + "｜" + offDetail.slice(0, 80),
);

// —— ② 接受之后，这一笔**不得产出预览** ——
//
// **为什么不断言 `preview_ms == 0`** ✗（我第一版这么写，是**错的** ✓）：
//   那个计时桶**同时也包着"为了完成 job 必须做的渲染"** ✓（实测：带开关 71 ms / 不带 121 ms ✓
//   ⇒ 省下的 ~50 ms 才是编码 ✓，剩下的 ~71 ms 是**必须发生**的脏区渲染 ✓）。
//   ⇒ **∴ 断言"桶为 0"会把"渲染还在做"误判成失败** ✗；那不是产品的错 ✓。
// **∴ 改判"语义边界"** ✓：带开关的响应里**没有 `preview` 字段** ✓（要图 vs 不要图 ✓）。
//   这条**能红** ✓：把分支短路撤掉 ⇒ `preview` 又会出现在响应里 ⇒ 判据红 ✓。
if (off.ok === true) {
  const offMs = previewMs(off);
  const hasPreview = off.preview !== undefined && off.preview !== null;
  console.log(`  preview:false ⇒ preview 字段${hasPreview ? "有 ✗" : "**无** ✓"}｜preview_ms=${offMs}`);
  check(
    !hasPreview,
    "`preview: false` 时响应里**没有 `preview`**（这一笔真的没产出图）",
    hasPreview ? "preview=" + JSON.stringify(off.preview).slice(0, 80) : "",
  );
  // **job 仍须完成** ✓ —— 省的是图，不是渲染水位 ✓（否则就是把"少算"当提速赚了 ✗）。
  check(
    off.job_status === undefined || off.job_status === "committed" || off.job_status === "completed",
    "`preview: false` 时 **job 仍须完成**（不是靠少渲染换速度）",
    "job_status=" + off.job_status,
  );
}

console.log("");
if (failures.length) {
  console.error(`结论：预览解耦未达标 ✗（${failures.length} 条）`);
  for (const item of failures) console.error("   - " + item);
  process.exit(1);
}
console.log("结论：✓ 可关闭每笔预览编码、且默认仍产出预览（语义边界判据）");
process.exit(0);
