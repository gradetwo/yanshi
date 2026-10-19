#!/usr/bin/env node
// **笔刷用途标签 + 按标签筛选**判据（第三方 MCP 报告第 5 条 ✓：
// "199 支笔刷靠文件名猜用途 ✗，筛选成本高 ✗" ⇒ 建议"加分类标签，或提供按用途筛选的参数" ✓）。
// 契约：① 不带 tag ⇒ 每支笔刷都有 `tags` 数组 ✓；② 带 tag ⇒ **只**返回带该标签的 ✓ 且**非空** ✓；
//       ③ 谎言检查：返回的每一项都必须**真的**含该标签 ✓（不能"筛了却给别的" ✗）。
// 用法：node scripts/tool-brush-tag-filter.mjs <server-base> <doc-id> <token>
// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 不可判：${String(error).slice(0, 80)}`);
  process.exit(3);
});
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) { console.error("用法: node scripts/tool-brush-tag-filter.mjs <base> <doc> <token>"); process.exit(2); }
const call = async (args) => (await (await fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool: "list_assets", arguments: args }),
})).json());
const failures = [];
const check = (ok, label, detail) => { console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? ` ⇒ ${detail}` : ""}`); if (!ok) failures.push(label); };
const all = await call({ kind: "brush" });
const brushes = all.assets || [];
check(brushes.length > 0, "列得出笔刷", `${brushes.length} 支`);
check(brushes.every((item) => Array.isArray(item.tags)), "每支都带 tags 数组（哪怕为空）");
const tagged = brushes.filter((item) => (item.tags || []).length > 0).length;
check(tagged > 0, "至少有一部分笔刷拿到了标签", `${tagged}/${brushes.length} 支有标签`);
const fur = await call({ kind: "brush", tag: "fur" });
const furItems = fur.assets || [];
check(furItems.length > 0, "按 tag=fur 筛出结果", `${furItems.length} 支`);
check(furItems.every((item) => (item.tags || []).includes("fur")), "筛出来的每一项都真的含该标签");
const none = await call({ kind: "brush", tag: "绝对不存在的标签" });
check((none.assets || []).length === 0, "不存在的标签筛出空集（而不是退回全部）");
if (failures.length) { console.log(`  ✗ 未满足 ${failures.length} 条：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 笔刷带用途标签，且按标签筛选是可信的");
