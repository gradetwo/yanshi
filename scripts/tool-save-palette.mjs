#!/usr/bin/env node
// **`save_palette` 判据** ✓（需求文档 P1-5 ✓：调色板管理 ✓）。
// **先查清了现状** ✓：**读**的那一侧已经有了（`list_palette_colors` ✓ + 资产 `kind: palette` ✓），
// 但**写**的一侧没有 ✗ ⇒ 缺口正是"把一份调色板**存下来、按名字再取回**" ✓。
// 判据 ✓：存进去 ⇒ 取回来**逐项一致**（颜色分量与顺序都不能变 ✓）；再存一次同名 ⇒ **覆盖而不是报错** ✓。
const base = process.argv[2];
const docId = process.argv[3] || "pal1";
if (!base) { console.error("用法: node scripts/tool-save-palette.mjs <base-url> [docId]"); process.exit(2); }
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 120, height: 90 }),
})).json();
const token = created.token;
if (!token) { console.error("判据无效：拿不到 token"); process.exit(1); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const colors = [
  { r: 32, g: 64, b: 160, a: 255 },
  { r: 240, g: 224, b: 192, a: 255 },
  { r: 200, g: 80, b: 40, a: 128 },
];
const name = "judge_roundtrip";
const saved = await call("save_palette", { name, colors });
if (saved.ok !== true) {
  console.log("  save_palette ⇒ " + JSON.stringify((saved.context || {}).detail || saved.error_code || saved).slice(0, 160));
  console.log("  ✗ 存不下来 ⇒ 判据红（预期 ✓ —— 这个工具还不存在 ✓）");
  process.exit(1);
}
console.log("  save_palette ok ✓｜" + JSON.stringify(saved).slice(0, 140));
const read = await call("list_palette_colors", { palette: name });
const back = read.colors || (read.data || {}).colors || [];
console.log("  取回：" + JSON.stringify(back).slice(0, 160));
const failures = [];
if (read.ok !== true) failures.push("取回失败 ⇒ " + JSON.stringify((read.context || {}).detail || read.error_code));
else if (back.length !== colors.length) failures.push(`取回 ${back.length} 项 ≠ 存入 ${colors.length} 项`);
else {
  const near = (a, b) => Math.abs((a ?? 0) - (b ?? 0)) <= 1;
  for (let i = 0; i < colors.length; i += 1) {
    const got = back[i] || {};
    if (!near(got.r, colors[i].r) || !near(got.g, colors[i].g) || !near(got.b, colors[i].b) || !near(got.a ?? 255, colors[i].a)) {
      failures.push(`第 ${i} 项不一致：存入 ${JSON.stringify(colors[i])} / 取回 ${JSON.stringify(got)}`);
    }
  }
}
// 再存一次同名 ⇒ 必须**覆盖**而不是报错 ✓（用户会反复改一版调色板 ✓）
const again = await call("save_palette", { name, colors: colors.slice(0, 1) });
if (again.ok !== true) failures.push("同名再存一次失败 ⇒ 应当覆盖而不是报错");
else {
  const reread = await call("list_palette_colors", { palette: name });
  const now = reread.colors || (reread.data || {}).colors || [];
  if (now.length !== 1) failures.push(`同名覆盖后应只剩 1 项，实际 ${now.length} 项`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ save_palette：存⇒取往返一致，且同名可覆盖");
