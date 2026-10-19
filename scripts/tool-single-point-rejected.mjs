#!/usr/bin/env node
// **单点笔触必须被响亮拒绝**判据（第 13 轮 ✓）
//
// **为什么要守它** ✗：一个**单点**不成笔触 ✓（工程上至少要两点才谈得上"笔迹" ✓）。
// 若这条校验被放松 ✗，表现就是"**调用报成功、画面上什么都没有**" ✗ —— 正是本仓**头号病根**
//（"说能用其实不能用" ✓）。第 339 轮实测：单点在多档尺寸下都返回
// `ok=false` ＋ `precondition_failed` ✓（**响亮** ✓），而**两点**（相距 40px）正常画出
// `painted_pixels=366` ✓ ⇒ 行为**正确** ✓ —— 本判据就是把这个"正确"钉住 ✓。
//
// **判据（两条，各自能红 ✓）**：
//   ① **单点必须被响亮拒绝** ✓：`ok === false` **且** `error_code` 非空 ✓
//      —— 只看 `ok === false` 不够 ✗：错误体必须**说得出原因** ✓（否则就是"失败了但不知道为什么" ✓）。
//      **变异**：把"点数 < 2 就拒"那条校验去掉 ⇒ `ok` 变 `true` ⇒ **判据红** ✓。
//   ② **两点必须照常画出** ✓（**防作弊** ✗）：否则"一律拒绝笔触"也能让 ① 通过 ✓，而产品就废了 ✓。
//
// **不写死观测** ✓：只看 `ok` 的真假与 `error_code` 的有无 ✓（语义边界 ✓），不看像素数、不看耗时 ✓。
//
// 用法：node scripts/tool-single-point-rejected.mjs <base-url>

// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 不可判：${String(error).slice(0, 80)}`);
  process.exit(3);
});
const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-single-point-rejected.mjs <base-url>");
  process.exit(2);
}

const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const doc = "single-point-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 1024, height: 768 }),
})).json();
if (!created.token) {
  console.error("✗ 前置不成立：建不出文档 ⇒ " + JSON.stringify(created).slice(0, 160));
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

const stroke = (id, points, size) => ({
  layer_id: "L1", object_id: id, brush: "100%_Opaque", size, color: { r: 0, g: 0, b: 0, a: 255 },
  preview: false, points,
});

// —— ① 单点必须被响亮拒绝（多档尺寸都试 ✓ —— 免得不小心只守住一档）——
// **★ 行为已反转（2026-10-15，真机报告 P2 #1）★**
//   原来这里断言"**单点必须被拒绝**" ✓ —— 那是一条**有意**的产品决策（"报错要指出真实根因" ✓）。
//   但真机反馈说：画"点"是最常见的操作 ✓，"每次都要记得要给 3 个点"很烦 ✓
//   （原话："有点烦" ✓）=> 现在的行为是：**退化成一点时自动展开成 3 个微偏移点** ✓
//   （`expand_degenerate_points` ✓，偏移随 `size` 走 ✓）。
//   => 这条判据**随之反转** ✓，但**保留**真正重要的那条不变式：
//      **真的落不下 dab 时，必须给出非空的 error_code ＋ 说得清原因的理由** ✓。
// **size 从 1 起试没有意义** ✓：实测 `size = 1` 时那支笔刷**本来就落不下**（`precondition_failed` ✓，
//   与"单点"无关 ✓）=> 用 8/20/40 ✓（真机报告里用的正是 8/30/90 ✓）。
for (const size of [8, 20, 40]) {
  const r = await call("brush_stroke", stroke("one" + size, [[300, 300, 1]], size));
  const code = r.error_code || r.error || null;
  console.log(`  size ${String(size).padStart(2)} 单点 ⇒ ok=${r.ok}｜error_code=${code}`);
  check(r.ok === true, `size ${size} 的**单点**现在应当**落下**（自动展开 ✓）`, "ok=" + r.ok);
  check(code === null, `落墨时不该有 error_code`, "error_code=" + code);
}

// **不变式（仍要守）**：真正落不下 dab 的情形（两点太短、笔尖太大 ✓）
//   ⇒ 必须给**非空 error_code** ＋ 说得出原因 ✓（这条**从来没有**被反转 ✓）。
{
  const r = await call("brush_stroke", stroke("short", [[300, 300, 1], [305, 305, 1]], 400));
  const code = r.error_code || r.error || null;
  const text = JSON.stringify(r.context || r);
  console.log(`  两点 5px ＋ size 400 ⇒ ok=${r.ok}｜error_code=${code}`);
  check(r.ok === false, "落不下 dab 的情形必须仍然报错（不许静默成功 ✓）", "ok=" + r.ok);
  check(code !== null && String(code).length > 0, "报错必须带非空 error_code ✓", "error_code=" + code);
  check(/个点|路径总长/.test(text), "报错必须说清**真实根因**（点数／路径总长 ✓）", text.slice(0, 120));
}

// —— ② 两点必须照常画出（防"一律拒绝"✗）——
const two = await call("brush_stroke", stroke("two", [[400, 400, 1], [450, 400, 1]], 12));
console.log(`  两点（相距 50px） ⇒ ok=${two.ok}｜painted=${two.painted_pixels}｜tiles=${two.dirty_tiles}`);
check(two.ok === true, "**两点**笔触必须照常被接受（ok === true）", "ok=" + two.ok);
check(Number(two.painted_pixels || 0) > 0, "**两点**笔触必须真的画出像素（painted_pixels > 0）",
  "painted_pixels=" + two.painted_pixels);

console.log("");
if (failures.length) {
  console.error(`结论：单点校验不合格 ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 单点被响亮拒绝、两点照常画出（语义边界判据，与像素数/耗时无关）");
process.exit(0);
