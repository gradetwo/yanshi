#!/usr/bin/env node
// **`import_project` 必须报出「实际恢复了什么」** ✓（外部 §84 ✓）
//   ⇒ **∴ 他们实测**✗**：`import_project` 返回 `ok=true` ✗ 而 `list_layers` 返回 `[]`**✗**
//   ⇒ **∴ 于是**每一笔都报「图层 paint 不存在」✗ 而**笔数 0**✗**
//   ⇒ **∴ 所以**：**∴ 返回值里必须有** `layers`**✗** ⇒ **∴ 否则**调用方**要**再查一次**✗ 才发现**✗** ✓
const base = process.argv[2];
const docId = process.argv[3] || "p1";
if (!base) { console.error("用法: node scripts/tool-import-project-layers.mjs <base-url> [docId]"); process.exit(2); }
const failures = [];
// **临时目录要用统一的 `trackTemp`** ✓（**∴ 第 253 轮规矩 ✓**）⇒ **∴ 否则**每跑一次**泄漏一个**✗**
import { trackTemp } from "./lib/temp-dirs.mjs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const dir = trackTemp(mkdtempSync(join(tmpdir(), "import-layers-")));
const pkg = `${dir}/p.yanshi`;

// **`?role=owner`**：下面第②步是**跨文档**导入（目标 `${docId}x`）⇒ 第三方审计 P2-N1 的
// 作用域闸会拒 editor 令牌（`ensure_doc_scope` ✓，与 MCP stdio 缺省对齐 ✓）。
const created = await (await fetch(`${base}/api/documents?role=owner`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 200, height: 200 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();

// **① 建两个具名图层 ＋ 画一笔** ✓
await call("create_layer", { layer_id: "ground", name: "g" });
await call("create_layer", { layer_id: "paint", name: "p" });
await call("brush_stroke", {
  layer_id: "paint", brush: "thin_watercolor", points: [[20, 20, 0.6], [120, 120, 0.6]], size: 12,
  color: { r: 30, g: 60, b: 200, a: 255 },
});
const exported = await call("export_project", { path: pkg });
if (exported.ok !== true) { console.log(`  ✗ 导出失败 ⇒ 判据无效 ⇒ ${JSON.stringify(exported).slice(0, 160)}`); process.exit(3); }

// **② 导入 ⇒ 返回值必须带 layers** ✓（§84 的核心）
const imported = await call("import_project", { path: pkg, doc_id: `${docId}x` });
console.log(`  导入 ⇒ ok=${imported.ok}｜layers=${JSON.stringify(imported.layers)}｜layers_count=${imported.layers_count}`);
if (imported.ok !== true) failures.push("导入失败 ✗");
if (!Array.isArray(imported.layers)) failures.push("返回值里没有 layers 数组 ⇒ 调用方要再查一次才发现图层丢了 ✗（§84）");
else {
  if (imported.layers.length < 3) failures.push(`layers 少了（实测 ${imported.layers.length} 个：${JSON.stringify(imported.layers)}）⇒ 结构没恢复全 ✗`);
  for (const want of ["ground", "paint"]) {
    if (!imported.layers.includes(want)) failures.push(`layers 里没有「${want}」⇒ 具名图层丢了 ✗`);
  }
  if (imported.layers_count !== imported.layers.length) failures.push("layers_count 与 layers 长度不一致 ✗（两者是同一个量 ✗）");
}
if (typeof imported.note !== "string" || !imported.note.includes("实际恢复")) failures.push("没有 note 说清 layers 是「实际恢复」的 ✗");

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ import_project 的返回值带 layers（含 ground 与 paint）与 layers_count");
