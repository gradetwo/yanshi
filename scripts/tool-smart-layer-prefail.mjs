// **P2-6**：`smart_*` 前置图层校验（fail-fast）+ 内层预览缺省关。
//   ① 不存在的 `layer_id` ⇒ **落笔前**就报 `reference_not_found`（错误文本带「落笔前校验」标记，
//      证明错误来自前置校验、不是算完才在 fold 里报；旧路径也会报同一错误码 ⇒ 只断错误码变异不红）。
//   ② 重输入（`smart_hatch_follow` 大排线）下也必须秒级失败 —— 宽松上界 15s，
//      对照旧行为：probe2 里 1259 笔算完才报，空烧 50s。校准（M-P6a 变异轮）：
//      20 引导线 × 200 点时旧路实测 14.5s，刚好在 15s 界内 ⇒ 墙钟臂不判别，
//      只有标记臂（error_code）能红 ⇒ 加重到 40 引导线 × 200 点，旧路约 29s，
//      让墙钟臂也判别；修好后前置校验在计算前就报，输入多大都是毫秒级。
//   ③ 内层预览缺省关：缺省调 `smart_fill` ⇒ `timings.preview_calls == 0`（语义计数，与墙钟无关）；
//      显式 `wait_for_render: true` ⇒ `preview_calls > 0`（证明这个参数真的被读了）。
// **前提失败必须 exit(3)**：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 3（不可判，不是产品失败）。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-smart-layer-prefail.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "slp_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 2000, height: 1600 }) })).json()).token;

const failures = [];
const detailOf = (made) => (made && made.context && made.context.detail) || "";

// ① **smart_fill，不存在的图层** ⇒ 秒级 `reference_not_found` + 前置校验标记 + 列出现有图层。
{
  const t0 = Date.now();
  const made = await call(doc, token, "smart_fill", {
    layer_id: "layer_nope",
    shape: { type: "rect", x: 10, y: 10, w: 400, h: 300 },
    color: [0.8, 0.2, 0.2, 1.0],
  });
  const ms = Date.now() - t0;
  const detail = detailOf(made);
  console.log(`  smart_fill 坏图层 ⇒ ok=${made && made.ok}｜error_code=${made && made.error_code}｜${ms}ms`);
  if (made.ok !== false) failures.push("smart_fill 坏图层应当失败");
  else if (made.error_code !== "reference_not_found") failures.push(`smart_fill 坏图层应当报 reference_not_found，实为 ${made.error_code}`);
  else if (!detail.includes("落笔前校验")) failures.push("smart_fill 的错误应当来自前置校验（文本带「落笔前校验」）：" + detail.slice(0, 120));
  else if (!detail.includes("layer_default")) failures.push("smart_fill 的错误应当列出现有图层：" + detail.slice(0, 120));
  if (ms > 15000) failures.push(`smart_fill 坏图层应当秒级失败（<15s），实为 ${ms}ms`);
}

// ② **smart_hatch_follow，大排线 + 不存在的图层** ⇒ 同样秒级失败（旧行为要先算完排线，50s+）。
{
  const guides = Array.from({ length: 40 }, (_, g) =>
    Array.from({ length: 200 }, (_, i) => [10 + (i * 1990) / 199, 10 + g * 37 + ((i * 7) % 13)]));
  const t0 = Date.now();
  const made = await call(doc, token, "smart_hatch_follow", {
    layer_id: "layer_nope",
    guides,
    width: 100,
    density: 1.0,
    size: 5,
    color: [0.2, 0.2, 0.2, 1.0],
  });
  const ms = Date.now() - t0;
  const detail = detailOf(made);
  console.log(`  smart_hatch_follow 大排线坏图层 ⇒ ok=${made && made.ok}｜error_code=${made && made.error_code}｜${ms}ms`);
  if (made.ok !== false) failures.push("smart_hatch_follow 大排线坏图层应当失败");
  else if (made.error_code !== "reference_not_found") failures.push(`smart_hatch_follow 大排线坏图层应当报 reference_not_found，实为 ${made.error_code}`);
  else if (!detail.includes("落笔前校验")) failures.push("smart_hatch_follow 的错误应当来自前置校验（文本带「落笔前校验」）");
  if (ms > 15000) failures.push(`smart_hatch_follow 大排线坏图层应当秒级失败（<15s，对照旧行为 50s+），实为 ${ms}ms`);
}

// ③ **内层预览缺省关** ⇒ 缺省 `smart_fill` 的 `preview_calls == 0`；显式 true ⇒ > 0。
{
  const args = {
    layer_id: "layer_default",
    shape: { type: "rect", x: 10, y: 10, w: 400, h: 300 },
    color: [0.8, 0.2, 0.2, 1.0],
  };
  const made = await call(doc, token, "smart_fill", args);
  const calls = made && made.timings && made.timings.preview_calls;
  console.log(`  smart_fill 缺省 ⇒ ok=${made && made.ok}｜preview_calls=${calls}`);
  if (made.ok !== true) failures.push(`smart_fill 正常调用应当成功：${JSON.stringify(made).slice(0, 200)}`);
  else if (calls !== 0) failures.push(`缺省时内层笔触不应产生预览记数（preview_calls 应 == 0，实为 ${calls}）`);
  const madeTrue = await call(doc, token, "smart_fill", { ...args, wait_for_render: true });
  const callsTrue = madeTrue && madeTrue.timings && madeTrue.timings.preview_calls;
  console.log(`  smart_fill wait_for_render:true ⇒ ok=${madeTrue && madeTrue.ok}｜preview_calls=${callsTrue}`);
  if (madeTrue.ok !== true) failures.push(`smart_fill 显式 true 时应当成功：${JSON.stringify(madeTrue).slice(0, 200)}`);
  else if (!(callsTrue > 0)) failures.push(`显式 wait_for_render:true 时内层笔触应当等渲染（preview_calls > 0，实为 ${callsTrue}）`);
}

if (failures.length) { console.log("  ✗ smart_* 前置校验 / 内层预览不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ smart_* 前置校验 / 内层预览：坏图层秒级报、缺省不等渲染");
process.exit(0);
