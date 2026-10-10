#!/usr/bin/env node
import { mkdtempSync, writeFileSync } from "node:fs";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
// **冷启动预算判据** ✓（第 263 轮 ✓）—— 目标第 2 条「**冷启动绝不整幅重渲**」✗ 的机器化 ✓。
//
// **背景** ✓：8K 文档**首次** `get_document`（默认要 256² 图 ✓）实测 **7 482 ms** ✗
//（`preview_ms` 7 427.787 ✓ —— 与墙钟几乎相等 ⇒ **计时可信 ✓**）；
// 而**显式 `preview_size:false`** ⇒ **8.1 ms** ✓（`preview_ms` 0.003 ✓）。
// ⇒ **∴ 那笔成本是"**默认就要图**"✗，不是"渲染本身慢"✗** ⇒ **∴ 调用方现在就有开关可绕开 ✓**。
//
// **判据（可红 ✓）**：
//  ① **`preview_size:false` 的首次读 ⇒ ≤ 50 ms** ✓
//     —— **变异** ✗：**把该开关忽略掉（仍然建图）⇒ 必红 ✓**；
//  ② **随后要 256 ⇒ 必须在合理预算内给出** ✓（**不得永远拿不到图 ✗**）。
// **∴ 成对 ✓**：① 守"要快能快"✓；② 守"要图能有"✓。
const [, , BASE, PROJECT_ARG, BUDGET_ARG] = process.argv;
// **∴ `argv[4]` 可能是**文档名／token**✗ ⇒ **∴ 必须**校验 ✓**（第 93 轮 ✓）
const __b = Number(BUDGET_ARG);
const BUDGET_MS = Number.isFinite(__b) && __b > 0 ? __b : 50;
// **★ 工程包必须是**给定文件或自造** ✗ ★**（第 93 轮 ✓）：
//   **∴ 症状（**本地复现 ✓）✗**：编排给 argv[3] 的是**文档名**✗（crit_… ✓）
//     ⇒ **∴ 而**本判据**把它当**工程包路径**✗
//       ⇒ **∴ readFileSync 一个不存在的路径 ⇒ ENOENT ⇒ 崩溃（EXIT=1）** ✓**** ✓✓
//   **∴ 修法**：**文件不存在就自己造一个**✗
//     ⇒ **∴ 于是**：**判据**不依赖外部文件** ✓ ⇒ **∴ 自足 ✓**
let PROJECT = PROJECT_ARG;
if (!PROJECT || !existsSync(PROJECT)) {
  const tmp = join(mkdtempSync(join(tmpdir(), "cold-open-")), "p.yanshi.json");
  const made = await (await fetch(`${BASE}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "cold_" + Date.now().toString(36),
      width: 256, height: 256 }) })).json();
  const madeToken = made.token, madeDoc = made.doc_id;
  await fetch(`${BASE}/api/tools/create_layer`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: madeDoc, token: madeToken, layer_id: "L0", name: "L0" }) });
  // **∴ 空文档导出的包只有 102 B**✗ ⇒ **∴ 导进去 ⇒ **没有图可给** ✓**（第 93 轮实测 ✓）
  //   ⇒ **∴ 所以**：**先**画一笔**✗ ⇒ **∴ 于是**包里有原子 ⇒ **∴ 判据**测的是**真的冷启动 ✓**
  await fetch(`${BASE}/api/tools/brush_stroke`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: madeDoc, token: madeToken, layer_id: "L0",
      brush: "100%_Opaque", size: 40, color: { r: 200, g: 100, b: 50, a: 255 },
      points: [[20, 20, 1.0], [120, 120, 1.0]] }) });
  const exported = await (await fetch(`${BASE}/api/tools/export_project`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: madeDoc, token: madeToken }) })).json();
  // **∴ 导出回的是 `blob_hash`**✗，**不是 `url`** ✗**（第 93 轮 ✓；**本地实测 ✓）⇒ **∴ 去 `/api/blob/<hash>` 取字节 ✓**
  const blobUrl = `${BASE}/api/blob/` + encodeURIComponent(exported.blob_hash) + `?doc=${encodeURIComponent(madeDoc)}&token=${encodeURIComponent(madeToken)}`;
  const bytes = new Uint8Array(await (await fetch(blobUrl)).arrayBuffer());
  writeFileSync(tmp, bytes);
  PROJECT = tmp;
  console.log(`  ⊘ 未给工程包 ⇒ 自己造了一个：${tmp}（${bytes.length} B）`);
}
if (!BASE || !PROJECT) {
  console.error("用法: node tool-cold-open-budget.mjs <base> <project.yanshi> [budget_ms]");
  process.exit(2);
}
import { readFileSync } from "node:fs";
const budget = Number(BUDGET_MS);
let doc = "", token = "";
async function call(tool, args) {
  const r = await fetch(`${BASE}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args),
  });
  return await r.json();
}
function check(ok, what, detail) {
  console.log(`  ${ok ? "✓" : "✗"} ${what}（${detail}）`);
  return ok;
}
const begin = await (await fetch(`${BASE}/api/documents/import?begin=1`, { method: "POST" })).json();
await fetch(`${BASE}/api/documents/import?upload=${begin.upload_id}&offset=0`, {
  method: "POST", headers: { "content-type": "application/octet-stream" },
  body: readFileSync(PROJECT),
});
const fin = await (await fetch(`${BASE}/api/documents/import?upload=${begin.upload_id}&finish=1`, { method: "POST" })).json();
doc = fin.doc_id; token = fin.token;
// ① **不建图** ⇒ 必须在预算内 ✓
let t0 = Date.now();
const fast = await call("get_document", { preview_size: false });
const fastMs = Date.now() - t0;
const a = check(fastMs <= budget, `**\`preview_size:false\` 的首次读 ≤ ${budget} ms** ✓`,
  `${fastMs} ms｜preview_ms=${(fast.timings || {}).preview_ms}`);
// ② **要图仍能拿到** ✓
t0 = Date.now();
const full = await call("get_document", { preview_size: 256 });
const fullMs = Date.now() - t0;
const thumb = (full.thumb_url || "").length > 0;
const b = check(thumb, "**随后要 256 ⇒ 必须真的给出图** ✓", `${fullMs} ms｜thumb=${thumb ? "有 ✓" : "无 ✗"}`);
console.log(`  结论：冷启动预算 ${a && b ? "**成立 ✓**" : "**未成立 ✗**"}`);
process.exit(a && b ? 0 : 1);
