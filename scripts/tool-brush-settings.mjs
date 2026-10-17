#!/usr/bin/env node
// **笔刷动力学读写判据** ✓（第 1000+ 轮）：`get_brush_settings` / `set_brush_setting`。
//
// **为什么必须有这条** ✗：引擎（`hokusai`）实现了 **63 项** MyPaint 动力学 ✓，而 MCP 侧此前只有
// `size`/`opacity`/`hardness`/`color` 四个旋钮 ✗。这两个工具是"agent 能不能控制笔速、手抖、
// 一笔内变色、厚涂涂抹"的唯一入口 ✓ ⇒ **它坏了，等于那 59 项又回到不可控** ✗。
//
// **四条断言**（每条都能红 ✓）：
//   ① `count` 必须等于引擎的权威项数 **63** ✓（**∴ 遍历写错、或硬编码成 43 ⇒ 红** ✓）；
//   ② 已知项必须读得到（`smudge` 的 `base_value` 是数字 ✓）；
//   ③ **设一项 ⇒ 读回一致** ✓，且**另一项不变** ✓（**∴ setter 写错键 ⇒ 红** ✓）；
//   ④ **未知设置名必须报错** ✓（**∴ 静默成功 ⇒ 红** ✓）。
//
// 用法：node scripts/tool-brush-settings.mjs <server-base> <doc-id> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-brush-settings.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return { status: response.status, body: await response.json() };
};
// 挑一支真实存在的笔刷（不硬编码名字 ✗ —— 资产会变 ✓）
let brush = null;
{
  const listed = await call("list_brushes", {});
  const names = (listed.body?.brushes ?? listed.body?.assets ?? []).map((entry) =>
    typeof entry === "string" ? entry : entry?.name,
  ).filter(Boolean);
  if (!names.length) {
    console.log("  ✗ list_brushes 没给出任何笔刷（判据无法继续）");
    process.exit(3);
  }
  brush = names[0].replace(/\.myb$/, "");
}
let bad = 0;
const check = (ok, why) => {
  console.log(`  ${ok ? "✓" : "✗"} ${why}`);
  if (!ok) bad += 1;
};
// ① 项数必须是 63
const before = await call("get_brush_settings", { brush });
if (before.status !== 200) {
  console.log(`  ✗ get_brush_settings 失败（HTTP ${before.status}）：${JSON.stringify(before.body).slice(0, 200)}`);
  process.exit(1);
}
check(before.body.count === 63, `项数 = ${before.body.count}（引擎权威项数是 63）`);
// ② 已知项读得到
const smudge = before.body.settings?.smudge?.base_value;
check(typeof smudge === "number", `smudge 的 base_value 是数字（读到 ${smudge}）`);
const radius = before.body.settings?.radius_logarithmic?.base_value;
check(typeof radius === "number", `radius_logarithmic 的 base_value 是数字（读到 ${radius}）`);
// ③ 设一项 ⇒ 读回一致；另一项不变
const target = Math.abs((smudge ?? 0) - 0.375) < 1e-9 ? 0.625 : 0.375;
const written = await call("set_brush_setting", { brush, setting: "smudge", value: target });
check(written.status === 200 && written.body.ok === true, `set_brush_setting 成功（HTTP ${written.status}）`);
const after = await call("get_brush_settings", { brush });
const smudgeAfter = after.body?.settings?.smudge?.base_value;
const radiusAfter = after.body?.settings?.radius_logarithmic?.base_value;
check(Math.abs((smudgeAfter ?? NaN) - target) < 1e-6, `读回一致：smudge = ${smudgeAfter}（写入 ${target}）`);
check(
  Math.abs((radiusAfter ?? NaN) - (radius ?? NaN)) < 1e-6,
  `其它项没被牵连：radius_logarithmic = ${radiusAfter}（原 ${radius}）`,
);
// ④ 未知设置名必须报错
const bogus = await call("set_brush_setting", { brush, setting: "definitely_not_a_setting", value: 1 });
check(bogus.status !== 200, `未知设置名被拒（HTTP ${bogus.status}）`);
if (bad) {
  console.log(`  ✗ 笔刷动力学读写有 ${bad} 处问题`);
  process.exit(1);
}
console.log("  ✓ 笔刷动力学读写：63 项可读、单项可写且读回一致、其它项不受牵连、未知名被拒");
