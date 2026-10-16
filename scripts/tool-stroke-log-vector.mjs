#!/usr/bin/env node
// **★ 普通矢量笔触（draw_stroke）必须进 get_stroke_log ✗ ★**（2026-10-16 ✓）
//
// **∴ 来源 ✗**：第三方 skill 的 VECTOR-LOG ✓ ＋ **我本机独立复现** ✓（**先读规格再测** ✓）。
//   实测（修前 ✓）：
//     draw_stroke ⇒ **ok=true** ✓（atom_id …002、head:3、dirty_kind:"geometry" ⇒ 这一笔真的画上了 ✓）
//     brush_stroke ⇒ ok=true ✓
//     get_stroke_log ⇒ entries 里有笔刷那一笔 ✓、**没有矢量那一笔** ✗
//
// **∴ 为什么是缺陷 ✗**（不是"少个字段" ✓）：`get_stroke_log` 是**画布历史的可读窗口** ✓ ——
//   拿它做**重放／时间轴／撤销核对**的客户端会**静默漏掉所有矢量笔触** ✗
//   ⇒ **∴ 与本仓修过的两条同族**：「**回执／日志与事实不一致**」✗（笔落了墨 ✓，日志说没有 ✓）。
//
// **∴ 判什么（三条 ✓，且**先确认动作成功再断言后果** ✗ —— 我上一版探针就栽在这儿 ✓）**：
//   ① `draw_stroke` 必须 **ok=true** ✓（否则**退 3** ✓，不许拿"日志里没有"当证据 ✗）；
//   ② 它必须出现在 `get_stroke_log` 里 ✓（按 object_id 能对上 ✓）；
//   ③ 笔刷那一笔也必须在 ✓（**对照物** ✓ —— 否则"日志整个坏了"会冒充"矢量漏了" ✗）。
//
// **∴ 能红 ✓**：把日志读取改回"只认 payload.points" ⇒ ② 立刻红 ✓（本机修前实测就是这个形态 ✓）。
//
// 用法：node scripts/tool-stroke-log-vector.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-stroke-log-vector.mjs <base-url>");
  process.exit(2);
}
const DOC = "stroke_log_" + Date.now().toString(36);
const call = async (token, tool, args) =>
  (await fetch(base + "/api/tools/" + tool + "?doc=" + DOC + "&token=" + token, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();

const created = await (await fetch(base + "/api/documents", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: 320, height: 240 }),
})).json();
if (!created || !created.token) {
  console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
  process.exit(3);
}
const token = created.token;
await call(token, "create_layer", { layer_id: "L1", name: "L1" });

// ① 矢量笔触：**颜色在 data 里** ✓（我第一次把 color 放顶层 ⇒ ok=false ⇒ 那是**我的错** ✗）
const vector = await call(token, "draw_stroke", {
  layer_id: "L1", object_id: "vec1",
  data: { points: [[20, 20], [120, 90]], size: 8, color: { r: 10, g: 10, b: 10, a: 255 } },
});
const brush = await call(token, "brush_stroke", {
  layer_id: "L1", object_id: "br1", brush: "100%_Opaque", size: 10,
  color: { r: 200, g: 30, b: 30, a: 255 }, points: [[40, 60], [140, 130]],
});
console.log("  ① draw_stroke ⇒ ok=" + (vector && vector.ok) + "｜brush_stroke ⇒ ok=" + (brush && brush.ok));
if (!vector || vector.ok !== true) {
  console.error("❌ 判不了：draw_stroke 本身没成功（" + JSON.stringify(vector).slice(0, 140) + "）⇒ 退出码 3");
  process.exit(3);
}
if (!brush || brush.ok !== true) {
  console.error("❌ 判不了：brush_stroke 本身没成功 ⇒ 退出码 3");
  process.exit(3);
}

const log = await call(token, "get_stroke_log", {});
const text = JSON.stringify(log);
const seesVector = text.includes("vec1");
const seesBrush = text.includes("br1");
const entries = (log && (log.entries || log.items)) || [];
console.log("  ② get_stroke_log ⇒ " + entries.length + " 条｜vec1=" + seesVector + "｜br1=" + seesBrush);

const failures = [];
if (!seesBrush) failures.push("**对照物**（笔刷笔触 br1）也不在日志里 ✗ ⇒ 先查日志整体，别急着怪矢量 ✓");
if (!seesVector) {
  failures.push("**普通矢量笔触（vec1）没进 get_stroke_log** ✗ ⇒ 重放／时间轴／撤销核对会**静默漏掉它** ✓（VECTOR-LOG ✓）");
}
if (failures.length) {
  console.error("❌ " + failures.length + " 项没成立：");
  for (const item of failures) console.error("   · " + item);
  process.exit(1);
}
console.log("✓ 矢量笔触（draw_stroke）与笔刷笔触都在 get_stroke_log 里 ✓");
