#!/usr/bin/env node
// **「完全在画布外的笔迹必须报真因」判据** ✓（外部 §81 ✓）
//   ⇒ **∴ 它**要**三样**✗**：**∴ `ok=false`**✗／**∴ 错误码是** `invalid_argument`**✗**／
//   **∴ 消息里有**画布尺寸**✗ 与**笔迹范围**✗ 与**「**别去换笔刷**」**✗** ✓
//   ⇒ **∴ 而**不能**只说**「**路径撑不满 dab 间距**」✗**（**∴ 那是**合理但非真因**✗** ✓
const base = process.argv[2];
// **∴ doc id 要唯一**✗（**∴ 与别的判据撞车 ⇒ 建文档失败 ⇒ 假红 ✓**）
const docId = process.argv[3] || `oob-${process.pid}-${Date.now() % 100000}`;
if (!base) { console.error("用法: node scripts/tool-stroke-outside-bbox.mjs <base-url> [docId]"); process.exit(2); }
const failures = [];
// **∴ 任何异常都必须**说出来**✗**（**∴ 第 336 轮：判据的诚实在于退出码与输出 ✓**）
//   ⇒ **∴ CI 上实测**✗**：输出停在**「画布外 ⇒ …」**✗ 而**退出码 1**✗** ⇒ **∴ 看不到原因**✗** ✓
process.on("uncaughtException", (e) => { console.log(`  ✗ 判据自身抛错（不是产品问题）：${e && e.message}`); process.exit(3); });
process.on("unhandledRejection", (e) => { console.log(`  ✗ 判据自身被拒（不是产品问题）：${e && e.message}`); process.exit(3); });
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 400, height: 400 }),
})).json();
const token = created.token;
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
await call("create_layer", { layer_id: "l1", name: "l" });

const paint = (pts, size) => call("brush_stroke", {
  layer_id: "l1", brush: "thin_watercolor", points: pts, size,
  color: { r: 30, g: 60, b: 200, a: 255 },
});

// **① 完全在画布外** ✓ ⇒ **∴ 必须**报那条真因**✗**
const out = await paint([[900, 900, 0.6], [1000, 1000, 0.6]], 18);
const outText = JSON.stringify(out);
console.log(`  画布外 ⇒ ok=${out.ok}｜code=${out.error_code}｜${outText.slice(0, 200)}`);
if (out.ok !== false) failures.push("整条笔迹在画布外却成功了 ⇒ 不该落墨 ✗");
if (out.error_code !== "invalid_argument") failures.push(`错误码该是 invalid_argument（实测 ${out.error_code}）✗`);
if (!outText.includes("400×400")) failures.push("消息里没写画布尺寸 ⇒ 调用方不知道边界 ✗（§81）");
if (!/x \d+\.\.\d+/.test(outText)) failures.push("消息里没写笔迹范围 ⇒ 调用方不知道差多远 ✗（§81）");
if (!outText.includes("别去换笔刷")) failures.push("消息里没排除笔刷原因 ⇒ 会被引去换笔刷 ✗（§81）");
if (outText.includes("撑不满")) failures.push("消息里仍在说 dab 间距 ⇒ 那是非真因 ✗（§81）");

// **② 画布内** ✓ ⇒ **∴ 必须**成功**✗**（**∴ 否则**① 的报错可能只是"全都失败" ✓）
const inside = await paint([[20, 20, 0.6], [180, 180, 0.6]], 18);
console.log(`  画布内 ⇒ ok=${inside.ok}`);
if (inside.ok !== true) failures.push("同一支笔在画布内却失败 ⇒ 判据无效（① 的报错无法归因）✗");

if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 画布外的笔迹报真因（画布尺寸＋笔迹范围＋别换笔刷），画布内的成功");
