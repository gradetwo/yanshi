#!/usr/bin/env node
// **★ 混合变更集的撤销：协作原子不许把"画布撤销"整条搞坏 ✗ ★**（2026-10-16 ✓）
//
// **∴ 来源 ✗**：第三方那 90 KB 本地补丁里的一处 ✓ —— `revert_changeset_atoms` 收集目标原子时**加了**
//   `.filter(|atom| !atom.kind.is_collab())` ✓，注释写着：
//     "评论/标注没有可撤销的画布状态；跳过它们才能完整撤销混合变更集" ✓
//   ⇒ **∴ 反推**不带这一条时：**一个变更集里既有画笔原子又有评论原子（collab ✓）** ⇒
//     **撤销会坏** ✗（要么报错 ✓，要么**画布没被撤销干净** ✗）。
//   **∴ 事实核对 ✗**：`AtomKind::is_collab()` ＝ `Comment | Suggest | AcceptSuggestion | RejectSuggestion` ✓；
//     **评论**由工具 `comment{text}` 写 ✓（**是原子 ✓**）——**注意别跟 `create_annotation` 搞混 ✗**：
//     后者自述"**独立 append-only 通道，不进原子日志**" ✓ ⇒ 它**不在**变更集里 ✓。
//
// **∴ 判什么（三条 ✓，两本账 ✓）**：
//   ① 一个变更集里**真的**同时有画笔原子与评论原子 ✓（没混成就**退 3** ✓，不许当"通过" ✗）；
//   ② `revert_changeset` 必须 `ok=true` ✓；
//   ③ **画布必须逐字节回到"落笔之前"** ✓（这是"撤销干净"的唯一硬证据 ✓ ——
//      不是看回执说你撤了几条 ✓）。
//
// **∴ 能红 ✓**：把 `revert_changeset_atoms` 的那个 collab 过滤器去掉 ⇒ ①成立而 ②/③ 红 ✓
//   （本机实测过修前形态 ✓）。
//
// 用法：node scripts/tool-revert-changeset-mixed.mjs <base-url>
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (!base) {
  console.error("用法: node scripts/tool-revert-changeset-mixed.mjs <base-url>");
  process.exit(2);
}
const DOC = `mixed_${Date.now().toString(36)}`;
const W = 200;
const H = 150;

const call = async (token, tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${DOC}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const pixels = async (token) => {
  const reply = await call(token, "render_region", { region: { x: 0, y: 0, w: W, h: H }, raw: true });
  const url = reply && (reply.raw_url || reply.rawUrl);
  if (!url) return null;
  const absolute = String(url).startsWith("http") ? String(url) : `${base}${url}`;
  return Buffer.from(await (await fetch(absolute)).arrayBuffer());
};
const objectCount = async (token) => {
  const reply = await call(token, "list_objects", { layer_id: "L1" });
  const list = (reply && (reply.objects || reply.items)) || [];
  return Array.isArray(list) ? list.length : null;
};

const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: W, height: H }),
})).json();
if (!created || !created.token) {
  console.error("❌ 判不了：建不了文档 ⇒ 退出码 3");
  process.exit(3);
}
const token = created.token;
await call(token, "create_layer", { layer_id: "L1", name: "L1" });
const baseline = await pixels(token);
const objectsBefore = await objectCount(token);
if (!baseline) {
  console.error("❌ 判不了：拿不到基线像素 ⇒ 退出码 3");
  process.exit(3);
}

// ---------- ① 一个变更集里：画笔 ＋ 评论 ----------
const began = await call(token, "begin_changeset", {});
if (!began || began.ok !== true) {
  console.error(`❌ 判不了：begin_changeset 失败（${JSON.stringify(began).slice(0, 140)}）⇒ 退出码 3`);
  process.exit(3);
}
const stroke = await call(token, "brush_stroke", {
  layer_id: "L1", object_id: "mixed_stroke", brush: "100%_Opaque", size: 14,
  color: { r: 180, g: 40, b: 40, a: 255 }, points: [[40, 40], [150, 110]],
});
if (!stroke || stroke.ok !== true) {
  console.error(`❌ 判不了：画笔失败（${JSON.stringify(stroke).slice(0, 140)}）⇒ 退出码 3`);
  process.exit(3);
}
const commented = await call(token, "comment", { text: "这一笔的过渡可以再软一点" });
const committed = await call(token, "commit_changeset", {});
const changeset = (stroke.changeset_id || (committed && committed.changeset_id) || "").toString();
console.log(`  ① begin/commit 变更集 ⇒ brush={ok:${stroke.ok}, changeset:${String(stroke.changeset_id).slice(0, 18)}…}｜comment={ok:${commented && commented.ok}}｜commit={ok:${committed && committed.ok}}`);

// 组里到底有几条、都是什么 kind ✓（**没混成就退 3** ✓）
const log = await call(token, "get_log", {});
const atoms = (log && (log.atoms || log.items)) || [];
const group = Array.isArray(atoms)
  ? atoms.filter((atom) => atom && String(atom.changeset_id || "") === changeset)
  : [];
const kinds = group.map((atom) => String(atom.kind || atom.type || "?"));
console.log(`     变更集里 ${group.length} 条原子：${JSON.stringify(kinds)}`);
if (!changeset || group.length === 0) {
  console.error("❌ 判不了：拿不到变更集里的原子 ⇒ 退出码 3");
  process.exit(3);
}
const hasDraw = kinds.some((kind) => /draw|brush|stroke|image/i.test(kind));
const hasCollab = kinds.some((kind) => /comment|suggest/i.test(kind));
if (!hasDraw || !hasCollab) {
  console.error(`❌ 判不了：这个变更集**没混成**（画笔=${hasDraw} 协作=${hasCollab} ✓）⇒ 测不到那条路 ⇒ 退出码 3`);
  process.exit(3);
}
const painted = await pixels(token);
const objectsPainted = await objectCount(token);

// ---------- ②③ 撤销 ----------
const reverted = await call(token, "revert_changeset", { changeset_id: changeset });
const after = await pixels(token);
const objectsAfter = await objectCount(token);
console.log(`  ② revert_changeset ⇒ ok=${reverted && reverted.ok}｜${JSON.stringify(reverted && (reverted.undone_count ?? reverted.context ?? {})).slice(0, 120)}`);
console.log(`  ③ 画布：落笔后 ${painted ? painted.length : "-"} 字节 ⇒ 撤销后 ${after ? after.length : "-"} 字节（基线 ${baseline.length} ✓）｜对象 ${objectsBefore} ⇒ ${objectsPainted} ⇒ ${objectsAfter}`);

const failures = [];
if (!reverted || reverted.ok !== true) {
  failures.push(`**撤销本身就失败了** ✗（回执：${JSON.stringify(reverted).slice(0, 200)}）`);
}
if (!painted || painted.equals(baseline)) {
  failures.push("这一笔**没画上去**（落笔后与基线逐字节相同 ✗）⇒ 判据无效 ✗");
}
if (!after) failures.push("撤销后拿不到像素 ✗");
else if (!after.equals(baseline)) {
  failures.push("**画布没有逐字节回到落笔之前** ✗ ⇒ 混合变更集没被完整撤销 ✓（这正是那条 collab 过滤器要修的 ✓）");
}
if (objectsAfter !== null && objectsBefore !== null && objectsAfter !== objectsBefore) {
  failures.push(`对象数没回到落笔前：${objectsBefore} ⇒ ${objectsAfter} ✗`);
}

// **④ 评论必须**还在** ✓**（撤销画布**不该**顺手删掉评论 ✓ —— 它们是独立通道 ✓）
const logAfter = await call(token, "get_log", {});
const atomsAfter = (logAfter && (logAfter.atoms || logAfter.items)) || [];
const commentsAfter = Array.isArray(atomsAfter)
  ? atomsAfter.filter((atom) => atom && /comment/i.test(String(atom.kind || atom.type || ""))).length
  : 0;
console.log(`  ④ 撤销之后评论还剩 ${commentsAfter} 条 ✓（不该因为撤销一笔画就消失 ✓）`);
if (commentsAfter < 1) failures.push("撤销把**评论也删掉了** ✗ ⇒ 评论是独立通道，不该被画布撤销带走 ✓");

if (failures.length) {
  console.error(`❌ ${failures.length} 项没成立：`);
  for (const item of failures) console.error(`   · ${item}`);
  process.exit(1);
}
console.log(`✓ 混合变更集（画笔 ＋ 评论）整条撤销：ok=true ✓、画布逐字节回到落笔之前 ✓、对象数也回到 ${objectsBefore} ✓`);
