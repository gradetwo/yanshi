#!/usr/bin/env node
// **below 合成必须被复用**判据（第 96 轮 ✓）—— 设计里的 **C3** ✓（目标第 4 条）
//
// **为什么单独立一个脚本** ✗：**C3 今天必然是红的** ✓（缓存还没实现 ⇒ `below_reuse` 恒 0 ✓）
//   ⇒ 若把它塞进 `tool-render-cache.mjs` ✓，整条脚本会红 ✗ ⇒ **∴ 会掩盖那里 C1／C2／C4 的绿** ✓
//   ⇒ **∴ 单独一条 ＋ 登记为已知红** ✓（本仓惯例 ✓，**且写明转绿条件** ✓）。
//
// **判据（两条，成对 ✓）**：
//   ① **只改当前层（最上面那层 ✓）之后再渲染 ⇒ `below_reuse` 必须增加** ✓
//      —— **语义计数** ✓，**不看墙钟** ✗（`/health` 的 `below_reuse` ✓，第 95 轮加 ✓）；
//   ② **防作弊** ✓：同一对渲染的**输出必须变化** ✓ —— 否则"什么都没做"也能让 ① 成立 ✗。
//
// **变异** ✗（打在**被判的那一处** ✓）：
//   * 让"当前层变化"也**清空 below** ⇒ ① 红 ✓；
//   * 让 `below_reuse` **不自增**（如缓存永不命中）⇒ ① 红 ✓（**今天就是这种** ✓）。
//
// **转绿条件** ✓：实现 below 缓存（tile 粒度 ✓、六类例外整条不走 ✓）⇒ ① 变绿 ✓。
//
// 用法：node scripts/tool-below-reuse.mjs <base-url>

import { createHash } from "node:crypto";

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-below-reuse.mjs <base-url>");
  process.exit(2);
}
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};
const sha = (buf) => createHash("sha256").update(buf).digest("hex");

const doc = "br-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 256, height: 192 }),
})).json();
if (!created.token) {
  console.error("✗ 前置不成立：建不出文档 ⇒ " + JSON.stringify(created).slice(0, 160));
  process.exit(2);
}
const token = created.token;
const call = async (tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const belowReuse = async () => Number((await (await fetch(`${base}/health`)).json()).below_reuse ?? -1);
const rawBytes = async () => {
  const value = await call("render_region", { region: [0, 0, 256, 192], raw: true });
  let url = value.raw_url || value.thumb_url;
  if (!url) throw new Error("拿不到 raw_url ⇒ " + JSON.stringify(value).slice(0, 160));
  if (String(url).startsWith("yanshi://blob/")) {
    url = `${base}/api/blob/${String(url).split("/").pop()}?doc=${encodeURIComponent(doc)}`;
  } else if (String(url).startsWith("/")) {
    url = base + url;
  }
  const res = await fetch(String(url));
  if (!res.ok) throw new Error(`取 raw 失败 HTTP ${res.status} ⇒ ${url}`);
  return Buffer.from(await res.arrayBuffer());
};

// **两层**：L1 在下（below 的内容来源 ✓），L2 在上（**当前层** ✓）。
for (const layer_id of ["L1", "L2"]) {
  const r = await call("create_layer", { layer_id });
  if (r.ok !== true) {
    console.error("✗ 前置不成立：建不出图层 " + layer_id);
    process.exit(2);
  }
}
const bottom = await call("brush_stroke", {
  layer_id: "L1", object_id: "b1", brush: "100%_Opaque", size: 60,
  color: { r: 40, g: 80, b: 200, a: 255 }, preview: false,
  points: [[40, 140, 1], [216, 140, 1]],
});
if (bottom.ok !== true) {
  console.error("✗ 前置不成立：下层笔触未成功");
  process.exit(2);
}
// **第一次渲染**：把 below 建起来（内容 ＝ L1 ✓）。
const before = await rawBytes();
const r1 = await belowReuse();

// **只改当前层（L2 ✓）** ⇒ below（＝ L1 的合成 ✓）**不受影响** ⇒ **∴ 应被复用** ✓
const top = await call("brush_stroke", {
  layer_id: "L2", object_id: "t1", brush: "100%_Opaque", size: 40,
  color: { r: 240, g: 120, b: 30, a: 255 }, preview: false,
  points: [[40, 60, 1], [216, 60, 1]],
});
if (top.ok !== true) {
  console.error("✗ 前置不成立：上层笔触未成功");
  process.exit(2);
}
const after = await rawBytes();
const r2 = await belowReuse();
console.log(`  below_reuse：改当前层前 ${r1} → 后 ${r2}`);

// ① **主判据** ✓：只改当前层 ⇒ below 必须被复用（计数增加 ✓）
check(r2 > r1,
  "**只改当前层之后再渲染 ⇒ `below_reuse` 必须增加**（below 缓存须复用下方合成 ✓，语义计数 ✓，不看墙钟 ✗）",
  `${r1} → ${r2}`);
// ② **防作弊** ✓：这一对该真的改变了输出（否则"什么都没做"也能让 ① 成立 ✗）
check(sha(after) !== sha(before),
  "**这一对渲染的输出必须变化**（防「什么都没做」也能让上一条成立 ✗）",
  `${sha(before).slice(0, 12)} → ${sha(after).slice(0, 12)}`);

console.log("");
if (failures.length) {
  console.error(`结论：below **没有被复用** ✗（${failures.length} 条）—— 缓存尚未实现 ✓（预期红 ✓）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ below 被复用（只改当前层时下方合成未重算）");
process.exit(0);
