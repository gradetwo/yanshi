#!/usr/bin/env node
// **`scatter_strokes` 判据（加固版）** ✓（需求文档 P0-1.3 ✓）。
//
// **为什么加固** ✓（上一版的弱点 ✓）：上一版比较的是"**不透明像素个数**"✗，而 300×200 画布上
// 24 笔会把整幅涂满 ⇒ 两个数都是 60000 ⇒ **相等是平凡的** ✗（判据饱和 ⇒ 什么都没测到 ✗）。
// **现在** ✓：① 用**两个文档**（避免层叠歧义 ✓）撒**同一个 seed** ✓ ⇒ **逐字节比较 RGBA** ✓；
//          ② **自检**：两者都不能"全不透明"✓（否则比较无意义 ✗）⇒ 用**小区域 + 少笔数**保证不饱和 ✓；
//          ③ **区分度检查**：换一个 seed ⇒ 必须**不同** ✓（否则它根本没在随机 ✗）。
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-scatter-strokes.mjs <base-url>"); process.exit(2); }
const makeDoc = async (id) => {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 200, height: 150 }),
  })).json();
  if (!created.token) { console.error("判据无效：拿不到 token"); process.exit(1); }
  return created.token;
};
const call = async (id, token, tool, args) => (await fetch(`${base}/api/tools?doc=${id}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const render = async (id, token) => {
  const raw = await call(id, token, "render_region", { region: { x: 0, y: 0, w: 200, h: 150 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) { console.log("  ✗ 拿不到直通 RGBA"); process.exit(1); }
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
const paint = async (id, token, seed) => {
  await call(id, token, "create_layer", { layer_id: "layer_default", name: "l" });
  // **上色前的基线** ✓ —— 文档自带不透明底色 ⇒ "不透明像素数"根本测不到墨 ✗（自检实测 30000 = 总数 ✓）
  // ⇒ 必须按"**与基线不同的字节数**"来量 ✓。
  const before = await render(id, token);
  const result = await call(id, token, "scatter_strokes", {
    layer_id: "layer_default", seed,
    area: { x: 40, y: 30, w: 100, h: 80 },
    palette: [{ r: 200, g: 120, b: 60, a: 255 }, { r: 60, g: 90, b: 160, a: 255 }],
    count: 6, brush: "classic-brush", size_range: [4, 10], opacity_range: [0.3, 0.9], direction: "random",
  });
  if (result.ok !== true) {
    console.log("  scatter_strokes ⇒ " + JSON.stringify((result.context || {}).detail || result.error_code || result).slice(0, 150));
    process.exit(1);
  }
  const after = await render(id, token);
  return { bytes: after, strokes: result.strokes, changed: after.reduce((n, v, i) => (v !== before[i] ? n + 1 : n), 0) };
};
const tokenA = await makeDoc("sc_a");
const tokenB = await makeDoc("sc_b");
const tokenC = await makeDoc("sc_c");
const a1 = await paint("sc_a", tokenA, 7);
const b1 = await paint("sc_b", tokenB, 7);   // 同 seed ⇒ 必须逐字节相同
const c1 = await paint("sc_c", tokenC, 8);   // 换 seed ⇒ 必须不同
const total = a1.bytes.length;
const same = a1.bytes.equals(b1.bytes);
console.log(`  笔数=${a1.strokes}✓｜与基线的差异字节：A=${a1.changed} B=${b1.changed} C=${c1.changed}（总 ${total}）`);
console.log(`  同 seed(7) 两文档逐字节相同？ ${same ? "是 ✓" : "否 ✗"}｜换 seed(8) 与 seed(7) 不同？ ${a1.bytes.equals(c1.bytes) ? "否 ✗" : "是 ✓"}`);
const failures = [];
// **自检**：两个数都饱和 ⇒ 比较无意义 ✗（上一版正是栽在这里 ✓）
// **自检**：必须有墨（差异 > 0 ✓），但不能"整幅都变"✗（那说明连底色都被重写 ⇒ 比较退化 ✗）
if (a1.changed === 0 || b1.changed === 0) failures.push("撒了笔触却一个字节都没变 ⇒ 判据无效 ✗");
if (a1.changed >= total || b1.changed >= total) failures.push("整幅每个字节都变了 ⇒ 比较退化 ⇒ 判据无效 ✗");
if (!same) failures.push("同 seed 撒到两个文档，结果不是逐字节相同 ⇒ 不可复现 ✗");
if (a1.bytes.equals(c1.bytes)) failures.push("换了 seed 结果却完全一样 ⇒ 它没有真的随机 ✗");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ scatter_strokes：同 seed 逐字节可复现，换 seed 结果不同，且画布未被涂满（比较有意义）");
