// **长批次中途的预览判据**（测试报告 §一.4）：`batch` 在一个循环里跑完，
// 单线程服务下这期间别的写盘路径没有机会 ⇒ `render.png` 会冻结"整批时长"。
// 新增 `preview_every_n_strokes` / `preview_interval_ms` 让**循环内部**周期性放行一次预览。
// 判据（可计数 ⇒ 确定性 ✓）：① 每 4 个放行 ⇒ 12 个调用里应恰好 3 个子结果带预览；
//                          ② `silent: true` ⇒ 0 个带预览（对照组，证明计数不是恒真）；
//                          ③ 给了工作区路径时，顺便量 `render.png` 是否真的被写过（可选，跳过会打印）。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-batch-preview.mjs <base-url>"); process.exit(2); }
import { statSync } from "node:fs";
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "bp_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 240, height: 180 }) })).json()).token;

const strokeCall = (i) => ({ tool: "brush_stroke", arguments: { layer_id: "layer_default", brush: "100%_Opaque",
  points: [[10 + i * 4, 20, 0.8], [60 + i * 4, 80, 0.8]], size: 10, color: { r: 180, g: 40, b: 40, a: 255 } } });
// **只看具体字段** ✓：以前扫整串 JSON ✗ ⇒ 任何含 "preview" 字样的字段都会命中 ✓
// （CI 实测：silent 组也报 4/4 ✗）⇒ 改成"该字段存在且非 null" ✓。
const withPreview = (result) => {
  const value = (result && result.preview !== undefined) ? result.preview : undefined;
  return value !== undefined && value !== null;
};

const failures = [];
const workspace = process.env.YANSHI_WORKSPACE;
const renderPath = workspace ? `${workspace}/docs/${doc}/render.png` : null;
const mtimeBefore = renderPath ? (() => { try { return statSync(renderPath).mtimeMs; } catch { return null; } })() : null;

const streamed = await call(doc, token, "batch", { preview_every_n_strokes: 4,
  calls: Array.from({ length: 12 }, (_, i) => strokeCall(i)) });
const results = (streamed && streamed.calls) || (streamed && streamed.results) || [];
const previews = results.filter((r) => withPreview(r.result || r)).length;
console.log(`  每 4 个放行 ⇒ 12 个调用里带预览的子结果：${previews} 个（期望 3）｜ok=${streamed && streamed.ok}`);
if (!results.length) failures.push("batch 没返回逐项结果 ⇒ 判据无法成立（先看返回字段）");
else if (previews !== 3) failures.push(`带预览的子结果应为 3 个（12 / 4），实为 ${previews} 个`);

const quiet = await call(doc, token, "batch", { silent: true,
  calls: Array.from({ length: 4 }, (_, i) => strokeCall(20 + i)) });
const quietResults = (quiet && quiet.calls) || (quiet && quiet.results) || [];
const quietPreviews = quietResults.filter((r) => withPreview(r.result || r)).length;
console.log(`  对照组（silent: true）带预览：${quietPreviews} 个（期望 0）`);
if (quietResults.length && quietPreviews !== 0) failures.push(`silent: true 时不应带预览，实为 ${quietPreviews} 个`);

if (renderPath) {
  const mtimeAfter = (() => { try { return statSync(renderPath).mtimeMs; } catch { return null; } })();
  console.log(`  render.png：前 ${mtimeBefore} ⇒ 后 ${mtimeAfter}（工作区由 YANSHI_WORKSPACE 给出）`);
  if (mtimeBefore !== null && mtimeAfter !== null && mtimeAfter === mtimeBefore) {
    failures.push("render.png 没有被写过（mtime 未变）⇒ 中途预览没有落盘");
  }
} else {
  console.log("  ⊘ 没给 YANSHI_WORKSPACE ⇒ 跳过 render.png 的 mtime 检查（只判响应里的预览计数）");
}

if (failures.length) { console.log("  ✗ 长批次中途预览不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ 长批次中途预览：按笔数放行，silent 时为零");
process.exit(0);
