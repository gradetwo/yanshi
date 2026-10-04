// **混合模式清单判据**（测试报告 §三.4）：工具层原先**硬编码 7 个模式** ✗，
// 而渲染层能解析 10 个（并把清单放在 `BlendMode::NAMES` ✓）⇒ **工具会拒绝内核懂的模式** ✗。
// 判据三件事：
//   ① `color_dodge` 必须被接受 ✓（**新增的模式** ✓）；
//   ② `difference` 必须被接受 ✓（**它一直能解析、却不在工具白名单里** ✗ —— 这条守的正是"漂移" ✓）；
//   ③ 对照组：一个真正不存在的模式**必须报错** ✓（否则 ① ② 就可能是"什么都不校验" ✓）。
// 能红：把工具的 ALLOWED 改回硬编码 7 个 ⇒ ① ② 立刻红 ✓。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-blend-modes.mjs <base-url>"); process.exit(2); }
const call = async (doc, token, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) });
  return await r.json();
};
const doc = "blend_" + Date.now().toString(36);
const token = (await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 200, height: 160 }) })).json()).token;
const layer = (await call(doc, token, "list_layers", {})).layers?.[0]?.layer_id || "layer_default";

const failures = [];
const tryMode = async (mode) => {
  const value = await call(doc, token, "set_layer_blend", { layer_id: layer, mode });
  const ok = !!(value && value.ok !== false && !value.error);
  console.log(`  set_layer_blend(${mode}) ⇒ ${ok ? "接受" : "拒绝"}${ok ? "" : "：" + JSON.stringify(value).slice(0, 130)}`);
  return ok;
};
for (const mode of ["color_dodge", "difference"]) {
  if (!(await tryMode(mode))) failures.push(`内核支持的模式 ${mode} 被工具层拒绝 ⇒ 两层清单仍然漂移`);
}
if (await tryMode("definitely_not_a_mode")) failures.push("不存在的模式被接受了 ⇒ 说明校验形同虚设（前后两条也就不成立）");

if (failures.length) { console.log("  ✗ 混合模式清单不合格："); for (const f of failures) console.log("     - " + f); process.exit(1); }
console.log("  ✓ 混合模式清单：内核支持的两个模式都被接受，不存在的模式被拒绝");
process.exit(0);
