#!/usr/bin/env node
// 新建文档的**会话语义**判据（回归报告暴露的三条：尺寸回显 / 会话切换 / 默认图层）。
//
// 为什么值得一条判据：报告与我都栽在同一处 —— new_document 认了尺寸，但
//   ① 响应不回显 width/height（读到 null ⇒ 误判"尺寸被忽略"）；
//   ② session_document 仍指**老文档** ⇒ 之后所有测量量的都是老文档；
//   ③ 新文档**没有默认图层** ⇒ 旧脚本 layer_id 为 undefined，还被误诊成"笔刷画不出"。
// 业界（Photoshop/Krita/GIMP/MyPaint）新建图像都自带一个图层 ⇒ 这条也按那个来判。
//
// 用法：node scripts/tool-session-semantics.mjs <server-base>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-session-semantics.mjs <server-base>"); process.exit(2); }
const post = async (path, payload) => {
  const response = await fetch(base + path, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(payload || {}),
  });
  return response.json();
};
const created = await post("/api/documents", { doc_id: "sem0", width: 800, height: 600 });
const token = created.token;
if (!token) { console.error("没拿到 token ⇒ 判据无法运行（不是通过）"); process.exit(1); }
const tool = (name, args) => post("/api/tools?doc=sem0&token=" + token, { tool: name, arguments: args });
let bad = 0;
const check = (label, ok, detail) => {
  console.log((ok ? "  ✓ " : "  ✗ ") + label + (detail ? "｜" + detail : ""));
  if (!ok) bad += 1;
};

// ① HTTP 建文档要回显尺寸
check("HTTP 建文档回显 width/height",
  created.width === 800 && created.height === 600,
  "回的是 width=" + JSON.stringify(created.width) + " height=" + JSON.stringify(created.height));

// ② new_document 认尺寸、回显尺寸，并且**把会话切到新文档**
const made = await tool("new_document", { doc_id: "sem1", width: 640, height: 480 });
check("new_document 回显 width/height",
  made.width === 640 && made.height === 480,
  "回的是 width=" + JSON.stringify(made.width) + " height=" + JSON.stringify(made.height));
// **会话归属在客户端**（读代码确认：工具改不了客户端的会话归属）⇒ 判据**不能**要求工具去切 ✗，
// 而要要求它**把"接下来该怎么做"说清楚**（这正是本项目对错误/回执的一贯要求）。
check("new_document 回执指明会话仍在哪个文档",
  made.session_document === "sem0",
  "session_document=" + JSON.stringify(made.session_document) + "（应当如实报客户端当前所在文档）");
check("new_document 回执给出切换办法（可操作）",
  typeof made.next === "string" && made.next.includes("sem1"),
  "next=" + JSON.stringify(made.next));

// ③ 新文档自带默认图层 —— **要在"那个文档"上量** ✗（我先前把判据写成量会话文档了 ——
// 与报告同一种错：`new_document` 建的是**另一个**文档，`list_layers` 读的却是会话文档）。
// 服务端因此把默认图层 id 直接写进回执（可自证 ✓）；而"新文档上真有那个图层"要用
// **它自己的文档**来验 —— 也就是 HTTP 建文档那条路（它的 token 直接可用 ✓）。
check("new_document 回执给出默认图层", typeof made.default_layer === "string" && made.default_layer.length > 0,
  "default_layer=" + JSON.stringify(made.default_layer));

const layers = await tool("list_layers", {});
const count = layers.count !== undefined ? layers.count : (layers.layers || []).length;
const firstId = ((layers.layers || [])[0] || {}).layer_id;
check("HTTP 新建文档自带默认图层", count >= 1, "count=" + count + " first=" + JSON.stringify(firstId));
// **真调用**（上一轮这里是写死的 false 占位 ✗ —— 那种"判据"不会红也不会绿 ✓，等于没有 ✓）。
if (firstId) {
  const stroke = await tool("brush_stroke", { layer_id: firstId, brush: "100%_Opaque", size: 40,
    points: [[20, 20, 1], [60, 20, 1]] });
  check("默认图层上直接能落墨（旧脚本的核心假设）", stroke.ok === true,
    "detail=" + String((stroke.context || {}).detail || "").slice(0, 60));
} else {
  check("默认图层上直接能落墨（旧脚本的核心假设）", false, "没有图层可落笔");
}
console.log(bad ? "  结论：" + bad + " 条不成立 ✗" : "  结论：会话语义全部成立 ✓");
process.exit(bad ? 1 : 0);
