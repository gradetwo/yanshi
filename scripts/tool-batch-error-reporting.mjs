#!/usr/bin/env node
// **`batch` 不得静默吞掉子调用的失败** ✓（第 1000+ 轮 ✓；Muse 的 P0-3 ✓）
//
// **∴ 修前的实测 ✓**：`calls = [{create_layer}, {**no_such_tool**}]` ⇒ 带不带 `silent` 都 **`ok = True`** ✗，
//   返回里**没有** `failed_calls`／`failed_ops`／`detail` ✗ ⇒ **∴ agent 会以为全成了 ✓**
//   ⇒ **∴ 这是"假成功"✗，比缺信息更难发现 ✓**（**∴ 缺信息你会去找 ✗，假成功你会继续画 ✗**）。
// **∴ 修后 ✓**：返回带 `failed_calls` ✓（**∴ 空数组 = 全成功 ✓；非空 = 有失败 ✓**，每条含 index／name／error ✓）。
//
// **用法**：node scripts/tool-batch-error-reporting.mjs <base> <doc> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-batch-error-reporting.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
const bad = await call("batch", {
  calls: [
    { name: "create_layer", args: { layer_id: "p0c_good", name: "p0c_good" } },
    { name: "no_such_tool_xyz", args: {} },
  ],
});
const good = await call("batch", {
  calls: [{ name: "create_layer", args: { layer_id: "p0c_good2", name: "p0c_good2" } }],
});

let failures = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) failures += 1; };

const badList = bad.failed_calls;
if (!Array.isArray(badList)) {
  // **∴ 没有这个字段**就是 bug 本身**✗ ⇒ **∴ 退 1 ✓，不是退 3 ✗**（**∴ 退 3 会说"判据跑不了"✗，掩盖了问题 ✓**）
  console.log(`  ✗ batch 的返回里没有 failed_calls 数组（keys: ${Object.keys(bad).join(",")}）⇒ 子调用失败会被静默吞掉`);
  process.exit(1);
}
check(badList.length === 1, `坏 op 被发现（failed_calls 有 ${badList.length} 条，应为 1）`);
if (badList.length >= 1) {
  const one = badList[0];
  check(one.name === "no_such_tool_xyz", `失败条目的 name = ${one.name}`);
  check(one.index === 1, `失败条目的 index = ${one.index}（应为 1）`);
  const detail = JSON.stringify(one.error ?? {}).slice(0, 120);
  check(/不存在|no_such_tool_xyz/.test(detail), `失败条目带原因：${detail}`);
}
check(Array.isArray(good.failed_calls) && good.failed_calls.length === 0, `全成功的 batch ⇒ failed_calls 为空（${JSON.stringify(good.failed_calls)}）`);
if (failures) { console.log(`  ✗ batch 的错误可见性有 ${failures} 处不满足`); process.exit(1); }
console.log("  ✓ batch 的失败可见：坏 op 被点名（index/name/原因），全成功时为空");
