#!/usr/bin/env node
// **声明的参数面 vs 实现接受的参数面** —— 一条能红的对账判据。
//
// **为什么需要它**：实测到 `brush_preview` 的实现会读 hardness/opacity，而它的公开参数面里没有，
// 调用方只好"猜"；这类不一致正是"每个工具带可复制调用示例"要治的病。
//
// **做法**：① 从 `/api/tools` 取每个工具声明的参数名；② 用**一个不存在的参数**去调它，
// 工具层会把"可用参数"列出来（实测过：`不接受参数 hardness（拼写错误？）；可用参数：brush, si…`）；
// ③ 两边的**集合**必须相等。
// 若某个工具先报别的错（缺必填项等）⇒ 记为"未能对账"（**如实计数**，不算通过）。
const base = process.argv[2];
const token = process.argv[3];
if (!base || !token) { console.error("用法: node scripts/tool-param-parity.mjs <server-base> <token>"); process.exit(2); }
const catalogue = await fetch(`${base}/api/tools`).then((r) => r.json());
const tools = catalogue.tools || catalogue.data?.tools || [];
if (!tools.length) { console.error("没取到工具清单 ✗"); process.exit(2); }
const doc = "tp1";
await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 900, height: 640 }),
}).then((r) => r.json());
let checked = 0, mismatched = 0, unchecked = 0;
const rows = [];
for (const tool of tools) {
  const name = tool.name;
  const declared = new Set((tool.parameters || []).map((p) => p.name));
  // **按声明的类型造一份像样的参数** —— 否则工具先报"缺必填项"✗，那个"可用参数"提示根本不出现 ✗
  //（上一版就是这么得到"可对账 0 个"却报"全部一致"的**假绿** ✗ —— 判据必须先证明自己会红 ✓）。
  const argumentsForProbe = { __probe_unknown__: 1 };
  for (const parameter of tool.parameters || []) {
    if (parameter.name === "__probe_unknown__") continue;
    const type = String(parameter.type || "any").toLowerCase();
    argumentsForProbe[parameter.name] =
      type.includes("number") || type.includes("integer") ? 1
      : type.includes("bool") ? true
      : type.includes("array") ? []
      : type.includes("object") ? {}
      : "x";
  }
  const response = await fetch(`${base}/api/tools?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool: name, arguments: argumentsForProbe }),
  }).then((r) => r.json());
  const detail = (response.context || {}).detail || "";
  const match = detail.match(/可用参数：(.+)$/);
  if (!match) { unchecked += 1; continue; }
  const accepted = new Set(match[1].split(/[,，、]/).map((s) => s.trim()).filter(Boolean));
  declared.delete("__probe_unknown__");
  const missing = [...accepted].filter((p) => !declared.has(p));       // 实现收、声明没有
  const extra = [...declared].filter((p) => !accepted.has(p));         // 声明有、实现不收
  checked += 1;
  if (missing.length || extra.length) {
    mismatched += 1;
    rows.push({ name, missing, extra });
  }
}
console.log(`  对账完成：可对账 ${checked} 个 ✓、未能对账 ${unchecked} 个（先报了别的错）、不一致 ${mismatched} 个`);
for (const row of rows.slice(0, 12)) {
  console.log(`  ${row.name.padEnd(24)} 实现收了但没声明: [${row.missing.join(", ")}]｜声明了但不收: [${row.extra.join(", ")}]`);
}
// **零对账 = 失败** ✗（不然"什么都没查到"会被读成"全都对"✗ —— 这正是本项目最反对的假绿 ✓）。
if (checked === 0) {
  console.log("结论：**一个都没能对账** ✗ ⇒ 判据无效（不是「全部一致」✗）");
  process.exit(1);
}
console.log(mismatched ? `结论：${mismatched} 个工具的声明与实现不一致 ✗` : "结论：全部一致 ✓");
process.exit(mismatched ? 1 : 0);
