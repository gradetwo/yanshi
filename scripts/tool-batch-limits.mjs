// **P2-7**：batch 上限与分段 ✓。
//   ① 501 个调用 ⇒ 硬拒（`invalid_argument`，文案指路"拆成多个 batch" ✓）；
//   ② 500 个调用 ⇒ 通过，且 `segments_completed == 5`（500/100 ✓）、`stopped_early == false` ✓；
//   ③ 250 个调用 ⇒ `segments_completed == 2`（完整跑完的段数，向下取整 ✓）。
//   水位提前停（`stopped_early: true`）在这里**测不到**（真实阈值 RSS > 4 GiB /
//   MemAvailable < 512MB 在判据机上触发不了 ✓）⇒ 谓词由 Rust 单测守着，真实阈值靠大文档对照实测 ✓。
// **前提失败必须 exit(3)**：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 3（不可判，不是产品失败）。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-batch-limits.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "bl_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 240, height: 180 }) })).json()).token;
const listCalls = (n) => Array.from({ length: n }, () => ({ tool: "list_layers", arguments: {} }));

const failures = [];
const detailOf = (made) => (made && made.context && made.context.detail) || "";

// ① **501 个调用** ⇒ 硬拒 ✓。
{
  const made = await call(doc, token, "batch", { calls: listCalls(501) });
  const detail = detailOf(made);
  console.log(`  501 调用 ⇒ ok=${made && made.ok}｜error_code=${made && made.error_code}`);
  if (made.ok !== false) failures.push("501 个调用应当被硬拒");
  else if (made.error_code !== "invalid_argument") failures.push(`超限应当报 invalid_argument，实为 ${made.error_code}`);
  else if (!detail.includes("500") || !detail.includes("拆成多个 batch")) failures.push("超限文案应当给出上限并指路拆分：" + detail.slice(0, 120));
}

// ② **500 个调用** ⇒ 通过，5 段，不提前停 ✓。
{
  const made = await call(doc, token, "batch", { calls: listCalls(500) });
  console.log(`  500 调用 ⇒ ok=${made && made.ok}｜segments_completed=${made && made.segments_completed}｜stopped_early=${made && made.stopped_early}｜count=${made && made.count}`);
  if (made.ok !== true) failures.push(`500 个调用应当通过：${JSON.stringify(made).slice(0, 200)}`);
  else if (made.segments_completed !== 5) failures.push(`500 个调用的 segments_completed 应 == 5，实为 ${made.segments_completed}`);
  else if (made.stopped_early !== false) failures.push(`正常跑完时 stopped_early 应 == false，实为 ${made.stopped_early}`);
  else if (made.count !== 500) failures.push(`子结果数应 == 500，实为 ${made.count}`);
}

// ③ **250 个调用** ⇒ 2 个完整段 ✓。
{
  const made = await call(doc, token, "batch", { calls: listCalls(250) });
  console.log(`  250 调用 ⇒ ok=${made && made.ok}｜segments_completed=${made && made.segments_completed}`);
  if (made.ok !== true) failures.push(`250 个调用应当通过：${JSON.stringify(made).slice(0, 200)}`);
  else if (made.segments_completed !== 2) failures.push(`250 个调用的 segments_completed 应 == 2，实为 ${made.segments_completed}`);
}

if (failures.length) { console.log("  ✗ batch 上限 / 分段不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ batch 上限 / 分段：501 拒、500 过且 5 段、250 2 段");
process.exit(0);
