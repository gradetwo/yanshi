#!/usr/bin/env node
// **`export_book_pngs` 必须**按页序**各导出一页 PNG** ✓（外部复测 `QX-G02` 的第一步 ✓）
//   ⇒ **∴ 判据**✗：**3 个文档**✗（**∴ 各画一笔不同颜色、各不同尺寸 ✓**）⇒ **∴ 导出 ✓**
//   ⇒ **∴ 断言**✗：**文件数 = 3**✗**｜**∴ 命名有序**✗**｜**∴ 每页尺寸 = 该文档尺寸**✗** ⇒ **∴ 不许**合并成一页**✗**
//   ⇒ **∴ 变异**✗：**漏掉一页**✗ ⇒ **∴ 必红**✗** ✓
import { mkdtempSync, readdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-export-book-pngs.mjs <base-url>"); process.exit(2); }
const stamp = `${process.pid}-${Date.now() % 100000}`;
const outDir = trackTemp(mkdtempSync(join(tmpdir(), "book-pngs-")));
const failures = [];
const pages = [
  { doc: `bk0-${stamp}`, w: 200, h: 150, color: [200, 40, 40] },
  { doc: `bk1-${stamp}`, w: 180, h: 120, color: [40, 200, 40] },
  { doc: `bk2-${stamp}`, w: 240, h: 160, color: [40, 40, 200] },
];
const tokens = {};
for (const page of pages) {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: page.doc, width: page.w, height: page.h }),
  })).json();
  if (!created.token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
  tokens[page.doc] = created.token;
  const call = (tool, args, doc = page.doc) => fetch(`${base}/api/tools?doc=${doc}&token=${tokens[doc]}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  }).then((r) => r.json());
  await call("create_layer", { layer_id: "p", name: "p" });
  await call("brush_stroke", {
    layer_id: "p", brush: "thin_watercolor",
    points: [[page.w * 0.1, page.h * 0.1, 0.6], [page.w * 0.6, page.h * 0.6, 0.6]],
    size: 16, color: { r: page.color[0], g: page.color[1], b: page.color[2], a: 255 },
  });
}

const result = await (await fetch(`${base}/api/tools?doc=${pages[0].doc}&token=${tokens[pages[0].doc]}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool: "export_book_pngs", arguments: { doc_ids: pages.map((p) => p.doc), dir: outDir } }),
})).json();
console.log(`  export_book_pngs ⇒ ok=${result.ok}｜count=${result.count}｜dir=${result.dir}`);
if (result.ok !== true) failures.push(`工具失败 ⇒ ${JSON.stringify(result).slice(0, 150)}`);

// **① 文件数 = 文档数** ✓（**∴ 不许**合并成一页**✗**）
const files = readdirSync(outDir).filter((n) => n.endsWith(".png")).sort();
console.log(`  目录里 ${files.length} 个 PNG：${files.join("｜")}`);
if (files.length !== pages.length) failures.push(`文件数 ${files.length} ≠ 文档数 ${pages.length} ⇒ 有页没导出 ✗`);

// **② 命名有序** ✓（**∴ `p00`／`p01`／`p02` ✓**）
const expected = pages.map((_, index) => `p${String(index).padStart(2, "0")}.png`);
for (const name of expected) {
  if (!files.includes(name)) failures.push(`缺 ${name} ⇒ 命名不按页序 ✗`);
}

// **③ 每页尺寸 = 该文档尺寸** ✓（**∴ 从返回的 pages 里核对 ✓**）
const reported = result.pages ?? [];
for (const [index, page] of pages.entries()) {
  const got = reported[index];
  if (!got) { failures.push(`第 ${index} 页没出现在 pages 里 ✗`); continue; }
  if (got.doc_id !== page.doc) failures.push(`第 ${index} 页的 doc_id 不对（实测 ${got.doc_id}）✗`);
  if (got.width !== page.w || got.height !== page.h) {
    failures.push(`第 ${index} 页尺寸 ${got.width}×${got.height} ≠ 文档 ${page.w}×${page.h} ⇒ 用了别的页的尺寸 ✗`);
  }
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log(`  ✓ ${files.length} 页按页序导出，尺寸各自正确（${pages.map((p) => `${p.w}×${p.h}`).join("｜")}）`);
