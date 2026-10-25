#!/usr/bin/env node
// **批量顶层 `ok` 即全成**（A6 方案 ①，Wang 2026-10-25 裁定）：
//   顶层 `ok` 与 `all_ok` 一致 —— 整批全成才是 `ok:true`；部分失败 ⇒ `ok:false`
//   + `error` 指路 `failed_calls`；`swatch` 同款（`ok` == `all_ok`）。
//   （此前：顶层 `ok` 只表示「请求跑完了」，一批里 17 笔全被打回仍 `ok:true` ——
//    外部 agent 只读 `ok` 就以为成功了，`fail_count=1` 被漏看。）
//
// **判据**：
//   ① 一批里 1 笔失败 ⇒ 顶层 `ok:false` + `all_ok:false` + `fail_count==1`
//      + `error` 提到 `failed_calls` + 恒等式 `ok_count + fail_count == count`
//      （HTTP 上部分失败现在返回 **400** 状态码 —— `call` helper 照常读 JSON 体，
//       不把非 200 当传输失败；`fetch` 本来就不抛错，直接 `response.json()`）；
//   ② 全合法批 ⇒ `ok:true` + `all_ok:true`（HTTP 200）；
//   ③ swatch 部分失败 ⇒ 顶层 `ok:false` + `all_ok:false`（HTTP 400）；
//   ④ swatch 全成 ⇒ `ok:true` + `all_ok:true`。
//
// **变异**：把 batch 的 `ok` 恒写 `true` ⇒ ① 红；把 `error` 指路删掉 ⇒ ① 红；
//   把 swatch 的 `ok` 恒写 `true` ⇒ ③ 红；把 `fail_count` 恒写 0 ⇒ ① 的恒等式红。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-batch-ok-semantics.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  const body = await response.json();
  return { status: response.status, body };
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};
const fails = [];
const check = (ok, msg) => {
  if (!ok) fails.push(msg);
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
};

const layer = await call("create_layer", { layer_id: "L_batch_ok", name: "batch_ok" });
if (layer.body.ok !== true && !String(layer.body.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer.body).slice(0, 120)}`);
}

const legal = {
  layer_id: "L_batch_ok",
  medium: "oil",
  size: 6,
  points: [
    [80, 60, 0.8],
    [110, 70, 0.8],
  ],
};
// **`preview` 是 `medium_stroke` 没有的参数** ⇒ 必须被拒（而不是被忽略）
// —— 与 `tool-batch-reports-failed-subcalls.mjs` 同一个造失败办法。
const illegal = { ...legal, points: [[30, 60, 0.8], [60, 70, 0.8]], preview: false };

// ① 一批里 1 笔失败
const mixed = await call("batch", {
  calls: [
    { tool: "medium_stroke", arguments: illegal },
    { tool: "medium_stroke", arguments: legal },
  ],
});
const m = mixed.body;
if (m.ok !== false && m.failed_calls === undefined) {
  fail(`① batch（1 笔失败）⇒ ${JSON.stringify(m).slice(0, 160)}`);
}
check(m.ok === false, `① 部分失败 ⇒ 顶层 ok=${m.ok}（应为 false）`);
check(m.all_ok === false, `① 部分失败 ⇒ all_ok=${m.all_ok}（应为 false）`);
check(m.fail_count === 1, `① fail_count=${m.fail_count}（应为 1）`);
check(
  typeof m.error === "string" && /failed_calls/.test(m.error),
  `① error 指路 failed_calls：${JSON.stringify(m.error ?? null).slice(0, 90)}`,
);
check(
  typeof m.ok_count === "number" &&
    typeof m.fail_count === "number" &&
    typeof m.count === "number" &&
    m.ok_count + m.fail_count === m.count,
  `① 恒等式 ok_count+fail_count==count（${m.ok_count}+${m.fail_count}==${m.count}）`,
);
check(mixed.status === 400, `① 部分失败 ⇒ HTTP 状态=${mixed.status}（应为 400）`);

// ② 全合法批
const clean = await call("batch", {
  calls: [{ tool: "medium_stroke", arguments: legal }],
});
check(clean.body.ok === true, `② 全合法批 ⇒ 顶层 ok=${clean.body.ok}（应为 true）`);
check(clean.body.all_ok === true, `② 全合法批 ⇒ all_ok=${clean.body.all_ok}（应为 true）`);
check(clean.status === 200, `② 全合法批 ⇒ HTTP 状态=${clean.status}（应为 200）`);

// ③ swatch 部分失败：1 支不存在的笔刷 + 1 支真笔刷
const swatchMixed = await call("swatch_brush_strokes", {
  brushes: ["calligraphy", "no_such_brush_xyz.myb"],
  size: 24,
  preview_size: "single",
});
const s = swatchMixed.body;
const badItem = (s.swatches ?? []).find((x) => x.brush === "no_such_brush_xyz.myb");
if (!badItem || badItem.ok !== false) {
  console.log("  ⊘ 不可判：不存在的笔刷没进失败项（swatch 前提不成立）");
  process.exit(3);
}
check(s.ok === false, `③ swatch 部分失败 ⇒ 顶层 ok=${s.ok}（应为 false）`);
check(s.all_ok === false, `③ swatch 部分失败 ⇒ all_ok=${s.all_ok}（应为 false）`);
check(s.fail_count === 1, `③ swatch fail_count=${s.fail_count}（应为 1）`);
check(
  typeof s.ok_count === "number" && s.ok_count + s.fail_count === s.count,
  `③ swatch 恒等式 ok_count+fail_count==count（${s.ok_count}+${s.fail_count}==${s.count}）`,
);
check(swatchMixed.status === 400, `③ swatch 部分失败 ⇒ HTTP 状态=${swatchMixed.status}（应为 400）`);

// ④ swatch 全成
const swatchClean = await call("swatch_brush_strokes", {
  brushes: ["calligraphy", "100%_Opaque.myb"],
  size: 24,
  preview_size: "single",
});
check(swatchClean.body.ok === true, `④ swatch 全成 ⇒ 顶层 ok=${swatchClean.body.ok}（应为 true）`);
check(swatchClean.body.all_ok === true, `④ swatch 全成 ⇒ all_ok=${swatchClean.body.all_ok}（应为 true）`);
check(swatchClean.status === 200, `④ swatch 全成 ⇒ HTTP 状态=${swatchClean.status}（应为 200）`);

if (fails.length) {
  console.log(`\n  ✗ ${fails.length} 处不符（批量顶层 ok 即全成，A6 方案 ①）`);
  process.exit(1);
}
console.log("\n  ✓ 批量顶层 ok 即全成：batch / swatch 的部分失败与全成都按新语义");
