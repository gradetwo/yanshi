#!/usr/bin/env node
// **★ list_brushes 要能**真的列出笔刷**（P1 #9 ✓）★**（第 254 轮 ✓）
//
// **∴ 现状（**读代码 ✓）✗**：**`read_list_brushes()` 是**写死的 4 支内置笔刷**✗
//   （圆头／软圆／抖动／虚线 ✓）＋**一句 note ⇒ **∴ AI 看不到 `assets/brushes` 里那 **201 支 .myb** ✓ ★★
//   ＋ **∴ 而且它**不带 `ctx`**✗ ⇒ **∴ 压根**看不到工作区** ✓（**∴ 这就是根因 ✓）★★
//
// **∴ 为什么它和 P0 #4 是**一对** ✗**：**`brush_params` 能覆盖任意 `.myb` 字段 ✓
//   ⇒ **∴ 但 AI 得先**知道有哪些笔刷**✗ **和**字段的规范名是什么**✓
//     ⇒ **∴ 否则它只能猜名字／猜字段（**猜错就被拒 ✓）** ✓ ★★★
//
// **∴ 判什么（**四条 ✓）★**：
//   ① 返回的笔刷数**明显多于 4**（要求 ≥ 100 ✓ —— `assets/brushes` 有 201 支 ✓）★
//   ② 每一条都有 `params`（**没有就如实 `null` ＋ note ✗ 不许假装 ✓）★
//   ③ 回执里带 **`canonical_param_names`**（**`.myb` 的规范字段名全表 ✓）
//      ⇒ **∴ 至少要含** `smudge`／`opaque`／`radius_logarithmic`** ✓（**`brush_params` 靠它 ✓）★★
//   ④ `filter` 参数要**真的过滤**（**`filter:"oil"` ⇒ 数量变少 ✗ 且每条名字里都含 oil ✓）★★
//
// **∴ 变异（**能红的根据 ✓）✗**：**把 `list_brushes` 改回写死 4 支 ⇒ **① 立刻红** ✓ ★★★
//
// 用法：node scripts/tool-list-brushes.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-list-brushes.mjs <base-url>");
  process.exit(2);
}
const call = async (doc, token, tool, args) =>
  (
    await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })
  ).json();

const main = async () => {
  const doc = `lb_${Date.now().toString(36)}`;
  const created = await (
    await fetch(`${base}/api/documents`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ doc_id: doc, width: 256, height: 256 }),
    })
  ).json();
  if (!created || !created.token) {
    console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
    process.exit(3);
  }
  const token = created.token;
  const all = await call(doc, token, "list_brushes", {});
  if (!all || all.ok !== true) {
    console.error(`❌ 判不了：list_brushes 调用失败（${JSON.stringify(all).slice(0, 160)}）⇒ 退出码 3`);
    process.exit(3);
  }
  const list = all.brushes || [];
  console.log(`  ① 笔刷数：${list.length}（其中 ${list.filter((b) => b.params).length} 条带 params）`);
  console.log(`  ③ canonical_param_names：${(all.canonical_param_names || []).length} 个`);
  const filtered = await call(doc, token, "list_brushes", { filter: "oil" });
  const flist = (filtered && filtered.brushes) || [];
  console.log(`  ④ filter:"oil" ⇒ ${flist.length} 支`);

  const bad = [];
  if (list.length < 100) {
    bad.push(`① 只列出 ${list.length} 支笔刷 ⇒ 没把 assets/brushes 里的 .myb 列出来（要求 ≥100）`);
  }
  const missingParams = list.filter((b) => !b.params && !b.note && !b.error);
  if (missingParams.length) {
    bad.push(`② 有 ${missingParams.length} 条既没有 params 也没有说明（不许假裝有参数）`);
  }
  const names = all.canonical_param_names || [];
  for (const needed of ["smudge", "opaque", "radius_logarithmic"]) {
    if (!names.includes(needed)) {
      bad.push(`③ canonical_param_names 里没有「${needed}」⇒ brush_params 的字段名无从发现`);
    }
  }
  if (!filtered || filtered.ok !== true) {
    bad.push(`④ 带 filter 的调用失败（${JSON.stringify(filtered).slice(0, 140)}）`);
  } else if (flist.length >= list.length) {
    bad.push(`④ filter:"oil" 没有过滤（${flist.length} vs 全量 ${list.length}）`);
  } else if (flist.some((b) => !String(b.name || "").toLowerCase().includes("oil"))) {
    bad.push("④ filter:\"oil\" 的结果里有名字不含 oil 的条目");
  }

  if (bad.length) {
    console.error(`❌ ${bad.length} 项没成立：${bad.join("｜")}`);
    process.exit(1);
  }
  console.log(
    `✓ list_brushes 真能列笔刷：${list.length} 支（≥100）、带 ${names.length} 个规范字段名、filter 生效`,
  );
};

await main();
