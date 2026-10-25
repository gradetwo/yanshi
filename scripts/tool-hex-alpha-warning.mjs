#!/usr/bin/env node
// **P2-8：`#RRGGBBAA` 的半透明 alpha 被静默丢掉**（期 2 块 2）
//
// **⇒ 缺陷**：画笔／介质颜色只取 RGB ⇒ 8 位 hex 的 alpha 在此丢掉；B4 的回显只认
//   对象式 `a` 键与数组 4 元，hex 写法没信号 ✗。
//
// **⇒ 判据**（`brush_stroke` 与 `medium_stroke` 两路）：
//   ① 半透明 `#RRGGBBAA`（alpha < 255）⇒ 回执 `warnings` 里有 alpha 被忽略的信号 ✓；
//   ② 不透明 `#RRGGBBAA`（alpha = FF）⇒ 没有这条信号（没丢信息，不许误报）✓；
//   ③ 6 位 `#RRGGBB` ⇒ 没有这条信号 ✓。
//
// **变异**：`hex_alpha_dropped` 恒回 `None` ⇒ ① 红 ✓。
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-hex-alpha-warning.mjs <base>");
  process.exit(3);
}
const doc = `hex_alpha_${Date.now()}`;
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
let TOK = "";
{
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 400, height: 200 }),
  })).json();
  TOK = made.token;
  if (!TOK) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
}
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };
const hasAlphaNote = (result) => (result?.warnings ?? [])
  .some((w) => typeof w === "string" && w.includes("alpha") && w.includes("被忽略"));

await post("create_layer", { layer_id: "L", name: "L" });
const brush = async (color) => post("brush_stroke", {
  layer_id: "L", brush: "2B_pencil", size: 24,
  points: [[40, 60, 0.8], [200, 60, 0.8]], color,
  preview: false, wait_for_render: false,
});
const medium = async (color) => post("medium_stroke", {
  layer_id: "L", medium: "oil",
  points: [[20, 20, 1.0], [40, 30, 0.6], [60, 44, 0.3]],
  size: 22, color,
});

for (const [label, fn] of [["brush_stroke", brush], ["medium_stroke", medium]]) {
  const semi = await fn("#ff000080");
  if (semi?.ok !== true) bad(`${label} 半透明臂调用失败 ⇒ ${JSON.stringify(semi).slice(0, 160)}`);
  else if (hasAlphaNote(semi)) ok(`① ${label}：半透明 #RRGGBBAA 进 warnings（alpha 被忽略）`);
  else bad(`① ${label}：半透明 #RRGGBBAA 必须进 warnings ⇒ ${JSON.stringify(semi.warnings ?? null)}`);
  const opaque = await fn("#ff0000ff");
  if (opaque?.ok !== true) bad(`${label} 不透明臂调用失败`);
  else if (!hasAlphaNote(opaque)) ok(`② ${label}：不透明 FF 没有 alpha 警告（没丢信息）`);
  else bad(`② ${label}：不透明 FF 不许报 alpha 警告`);
  const hex6 = await fn("#ff0000");
  if (hex6?.ok !== true) bad(`${label} 6 位臂调用失败`);
  else if (!hasAlphaNote(hex6)) ok(`③ ${label}：6 位 hex 没有 alpha 警告`);
  else bad(`③ ${label}：6 位 hex 不许报 alpha 警告`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（hex alpha 警告）`);
  process.exit(1);
}
console.log("\n  ✓ hex alpha：半透明有信号，不透明／6 位无误报（两路一致）");
