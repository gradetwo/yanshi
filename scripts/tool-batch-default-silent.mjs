// **D1（期 2 块 4）**：`batch` 的 `silent` 缺省改为 `true` ⇒ 不写 `silent` 时子结果不带预览 ✓。
//   三臂：① 缺省（不写）⇒ 0 个带预览；② 显式 `silent: true` ⇒ 0 个；
//   ③ 显式 `silent: false` ⇒ 每个都带预览（"没把预览整个删掉"的护栏 ✓）。
//   恒等式：`ok_count + fail_count == 调用数`，且 `all_ok == (fail_count == 0)` ✓。
// **前提失败必须 exit(3)**：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 3（不可判，不是产品失败）。
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-batch-default-silent.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "bds_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 240, height: 180 }) })).json()).token;

// **只看具体字段**：预览"存在且非 null"才算带（扫整串 JSON 会误命中别的字段名）。
const withPreview = (result) => {
  const value = (result && result.preview !== undefined) ? result.preview : undefined;
  return value !== undefined && value !== null;
};
const shapeCall = (i) => ({ tool: "draw_shape", arguments: { layer_id: "layer_default", object_id: `s${i}`,
  data: { geometry: { kind: "rect", bbox: { x: 10 + i * 50, y: 10, w: 40, h: 30 } },
          color: { r: 10, g: 10, b: 10, a: 255 } } } });
const calls = Array.from({ length: 4 }, (_, i) => shapeCall(i));

const failures = [];
const checkBatch = async (label, extra, expectedPreviews) => {
  const made = await call(doc, token, "batch", { ...extra, calls });
  const results = (made && made.calls) || (made && made.results) || [];
  const previews = results.filter((r) => withPreview(r.result || r)).length;
  console.log(`  ${label} ⇒ 带预览的子结果：${previews} 个（期望 ${expectedPreviews}）｜ok=${made && made.ok}｜ok_count=${made && made.ok_count}｜fail_count=${made && made.fail_count}`);
  if (!results.length) failures.push(`${label}：batch 没返回逐项结果 ⇒ 判据无法成立（先看返回字段）`);
  else if (previews !== expectedPreviews) failures.push(`${label}：带预览的子结果应为 ${expectedPreviews} 个，实为 ${previews} 个`);
  if (results.length && made.ok_count + made.fail_count !== results.length) {
    failures.push(`${label}：恒等式破了（ok_count + fail_count 应 == ${results.length}，实为 ${made.ok_count} + ${made.fail_count}）`);
  }
  if (results.length && made.all_ok !== (made.fail_count === 0)) {
    failures.push(`${label}：all_ok 应 == (fail_count == 0)，实为 all_ok=${made.all_ok}、fail_count=${made.fail_count}`);
  }
  return made;
};

// ① **缺省（不写 silent）** ⇒ D1 之后缺省就是静默 ⇒ 0 个带预览 ✓。
await checkBatch("缺省", {}, 0);
// ② **`silent: true`** ⇒ 0 个 ✓。
await checkBatch("silent: true", { silent: true }, 0);
// ③ **`silent: false`** ⇒ 显式关掉 ⇒ 4 个都带预览 ✓ —— 这条是"没把功能删掉"的护栏 ✓。
await checkBatch("silent: false", { silent: false }, 4);

if (failures.length) { console.log("  ✗ batch 缺省静默不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ batch 缺省静默：不写 silent 就不带预览，显式 false 恢复");
process.exit(0);
