#!/usr/bin/env node
// **`batch` 的子调用必须**真的被校验**，且失败**可见**（第 1402 轮；外部丢 1495 个子调用换来的）
//
// **⇒ 原缺陷** ✗：`ToolRegistry::call`（HTTP 入口）里有 `validate_args`，而 `batch` **直接调 `dispatch`**
//   ⇒ **绕过了它** ⇒ **未知参数被静默忽略** ⇒ 而 `batch` 返回 `ok: true`、`failed_calls: []` ✗✗
//   **⇒ 实测**：`medium_stroke` 带 `preview`（它没有这个参数）
//     · 单独调用 ⇒ `ok=false`「不接受参数 preview」✓
//     · 放进 `batch` ⇒ `ok=true`、子调用 `ok=true`、`failed_calls=[]` ✗
//   ⇒ **⇒ 这正是本仓最反对的静默降级** ✓（外部因此画错 1495 笔而没有任何错误信号）
//
// **⇒ 断什么**（结构量 ✓）：
//   ① 带**未知参数**的子调用 ⇒ `failed_calls` **非空**，且错误里提到那个参数名
//   ② 那条**不提交**原子（`head` 只 +1，而不是 +2）
//   ③ 恒等式：`ok_count + fail_count === count`，且 `all_ok === (fail_count === 0)`
//   ④ 全合法的 batch ⇒ `all_ok === true`、`failed_calls` 空
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-batch-reports-failed-subcalls.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const doc = `bfail_${stamp}`;
const made = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 288, height: 512 }),
})).json();
const TOK = made.token;
if (!TOK) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const head = async () => {
  const r = await post("get_document", {});
  const v = r.head_seq ?? r.context?.head_seq;
  return typeof v === "number" ? v : null;
};

await post("create_layer", { layer_id: "L", name: "L" });
const h0 = await head();
if (h0 === null) { console.error("  ⊘ 不可判：读不到 head_seq"); process.exit(3); }

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const legal = { layer_id: "L", medium: "oil", size: 6, points: [[80, 60, 0.8], [110, 70, 0.8]] };
// **`preview` 是 `medium_stroke` 没有的参数** ⇒ 必须被拒（而不是被忽略）
const illegal = { ...legal, points: [[30, 60, 0.8], [60, 70, 0.8]], preview: false };

const r1 = await post("batch", { calls: [
  { tool: "medium_stroke", arguments: illegal },
  { tool: "medium_stroke", arguments: legal },
] });
const h1 = await head();
const fc = r1?.failed_calls ?? [];

// ① 失败可见
if (fc.length === 1) {
  const detail = JSON.stringify(fc[0]);
  if (/preview/.test(detail)) ok("① 未知参数被拒并留痕（failed_calls 1 条，提到 preview）");
  else bad(`① failed_calls 里应提到 preview ⇒ 实测 ${detail.slice(0, 120)}`);
} else {
  bad(`① 带未知参数的子调用必须失败 ⇒ 实测 failed_calls=${fc.length}（0 ⇒ 又被静默忽略了）`);
}

// ② 失败那条不提交原子
if (h0 !== null && h1 !== null) {
  const d = h1 - h0;
  if (d === 1) ok(`② 失败的那条不提交原子 ⇒ head 只 +1（${h0} → ${h1}）`);
  else bad(`② 只有合法的 1 笔该提交 ⇒ 实测 head +${d}`);
}

// ③ 恒等式
const okc = r1?.ok_count, failc = r1?.fail_count, cnt = r1?.count;
if (typeof okc === "number" && typeof failc === "number" && typeof cnt === "number") {
  if (okc + failc === cnt) ok(`③ 恒等式 ok_count+fail_count=count（${okc}+${failc}=${cnt}）`);
  else bad(`③ ok_count+fail_count 必须等于 count ⇒ 实测 ${okc}+${failc} ≠ ${cnt}`);
  if (r1.all_ok === (failc === 0)) ok(`③ 恒等式 all_ok === (fail_count === 0)（${r1.all_ok}）`);
  else bad(`③ all_ok 与 fail_count 矛盾 ⇒ all_ok=${r1.all_ok}｜fail_count=${failc}`);
} else {
  bad("③ 响应里缺 ok_count/fail_count/count");
}

// ④ 全合法 ⇒ all_ok
const r2 = await post("batch", { calls: [{ tool: "medium_stroke", arguments: legal }] });
if (r2?.all_ok === true && (r2.failed_calls ?? []).length === 0) ok("④ 全合法的批 ⇒ all_ok=true、failed_calls 空");
else bad(`④ 全合法批的 all_ok 应为 true ⇒ 实测 ${r2?.all_ok}｜failed ${(r2?.failed_calls ?? []).length}`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（batch 必须校验子调用并报告失败）`);
  process.exit(1);
}
console.log("\n  ✓ batch 的子调用校验与失败报告都成立");
