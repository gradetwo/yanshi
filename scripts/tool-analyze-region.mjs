#!/usr/bin/env node
// **`analyze_region` 判据** ✓（需求文档 P2-10 区域色彩分析 ✓）。
// 文档要的是 `{dominant_colors, avg_brightness, warm_cool_ratio}` ✓ —— 用来让 AI **先看再画** ✓。
// **现在必然红** ✓（工具不存在 ✓）—— 先写判据、先看红 ✓。
// 判据（用**已知**的输入 ✓，所以"正确答案"是确定的 ✓）：
//   ① 把一块区域填成**纯红** ✓ ⇒ 主色第一项必须是**红**（r 明显大于 g/b ✓）；
//   ② 把另一块填成**纯蓝** ✓ ⇒ 主色第一项必须是**蓝** ✓（换个方向再验一次 ✓，防"永远回红"✗）；
//   ③ `avg_brightness` 与 `warm_cool_ratio` 必须**在合理范围内** ✓（否则字段形同虚设 ✗）；
//   ④ 红块的 `warm_cool_ratio` 应当**高于**蓝块 ✓（暖冷比要有区分度 ✓）。
const base = process.argv[2];
const docId = process.argv[3] || "ar1";
if (!base) { console.error("用法: node scripts/tool-analyze-region.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 200, height: 150 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const paint = async (layer, color, region) => {
  const made = await call("create_layer", { layer_id: layer, name: layer });
  if (!made.ok) return "建层失败：" + JSON.stringify((made.context || {}).detail);
  const filled = await call("fill", { layer_id: layer, data: { color, region } });
  if (!filled.ok) return "填充失败：" + JSON.stringify((filled.context || {}).detail);
  return null;
};
// 左半红、右半蓝（**用已知输入** ✓ ⇒ 正确答案确定 ✓）
const left = { x: 0, y: 0, w: 100, h: 150 };
const right = { x: 100, y: 0, w: 100, h: 150 };
const err1 = await paint("red_layer", { r: 220, g: 20, b: 20, a: 255 }, left);
const err2 = await paint("blue_layer", { r: 20, g: 40, b: 220, a: 255 }, right);
if (err1 || err2) { console.log("  ✗ 铺底失败 ⇒ 判据无效 ⇒ " + (err1 || err2)); process.exit(1); }
const analyse = (region) => call("analyze_region", { region });
const red = await analyse(left);
const blue = await analyse(right);
if (red.ok !== true) {
  console.log("  analyze_region ⇒ " + JSON.stringify((red.context || {}).detail || red.error_code || red).slice(0, 160));
  console.log("  ✗ 工具没成功 ⇒ 判据红（预期 ✓ —— 它还不存在 ✓）");
  process.exit(1);
}
const pick = (value) => (value.dominant_colors || [])[0] || {};
const redTop = pick(red), blueTop = pick(blue);
console.log("  红区分析：" + JSON.stringify(red).slice(0, 200));
console.log("  蓝区分析：" + JSON.stringify(blue).slice(0, 200));
const failures = [];
if (!((redTop.r ?? 0) > (redTop.g ?? 0) + 30 && (redTop.r ?? 0) > (redTop.b ?? 0) + 30)) {
  failures.push(`红区的主色不是红：${JSON.stringify(redTop)}`);
}
if (!((blueTop.b ?? 0) > (blueTop.g ?? 0) + 30 && (blueTop.b ?? 0) > (blueTop.r ?? 0) + 30)) {
  failures.push(`蓝区的主色不是蓝：${JSON.stringify(blueTop)}`);
}
const brightness = (value) => value.avg_brightness;
if (typeof brightness(red) !== "number" || brightness(red) < 0 || brightness(red) > 1) {
  failures.push(`avg_brightness 不在 0..1 或不是数字：${JSON.stringify(brightness(red))}`);
}
const warm = (value) => value.warm_cool_ratio;
if (typeof warm(red) !== "number" || typeof warm(blue) !== "number") {
  failures.push("warm_cool_ratio 不是数字");
} else if (warm(red) <= warm(blue)) {
  failures.push(`暖冷比没有区分度：红区 ${warm(red)} ≤ 蓝区 ${warm(blue)}`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ analyze_region：主色与暖冷比都能对上已知输入");
