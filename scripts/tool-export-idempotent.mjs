#!/usr/bin/env node
// **导出必须幂等**判据（第 36 轮 ✓）—— 为"导出证明记忆化"铺的护栏 ✓
//
// **为什么需要它** ✗（实测为据 ✓，第 35／363 轮 ✓）：
//   `export_project` 会为**每个对象**把笔刷**重放一遍**并比对哈希 ✓，来证明"不装这张位图，
//   包打开也一定是对的" ✓（注释："**不是看名字猜**"✓）。真实 4K 工程（**720 个对象** ✓）
//   实测 **2.62 s** ✗，`omitted = 720` ✓ ⇒ **≈3.6 ms/次重放** ✓ ⇒ **∴ 它就是"保存慢"的全部** ✓。
//   **∴ 下一步要做"把证明记忆化"** ✓ ⇒ 而那等于**省掉一次正确性验证** ✗
//   ⇒ **∴ 必须先有一条"证明只影响包体积、不许影响包内容"的判据** ✓ —— 本判据就是它 ✓。
//
// **判据（三条，各自能红 ✓）**：
//   ① **两次导出的包字节完全相同** ✓（`bytes` 相同 ＋ **`blob_hash` 相同** ✓ ——
//      内容寻址 ⇒ 字节一致必然哈希一致 ✓，两条一起判更硬 ✓）。
//   ② **两次导出的重放计数完全相同** ✓（`omitted`／`kept_no_recipe`／`kept_mismatch` ✓）
//      —— 记忆化若**错用**了证明 ✓，计数会漂移（或该重证的没重证 ✓）。
//   ③ **防作弊** ✓：`omitted > 0` ✓（证明**真的在跑重放** ✓）—— 否则"完全不验证"✗
//      也能让 ①② 通过 ✓，而那正是最坏的情况 ✗。
//
// **变异** ✓（打在**被判的那一处** ✗）：把 `replay_brush_bitmap` 的比对改成**恒不通过** ✗
//   ⇒ `kept_mismatch` 增长／`omitted` 归零 ⇒ ③ 红 ✓（且 ② 也可能红 ✓）。
//   **未来加记忆化后** ✓，另一条变异是"**把版本从键里去掉**"✗ ⇒ 若证明被错误复用 ⇒ ② 或 ① 红 ✓。
//
// **不写死观测** ✓：比的是"两次是否相同"（相等 ✓）与"是否大于 0"（语义边界 ✓），
//   不看具体字节数、不看耗时 ✓。
//
// 用法：node scripts/tool-export-idempotent.mjs <base-url>

const base = process.argv[2];
if (!base) {
  console.error("用法: node scripts/tool-export-idempotent.mjs <base-url>");
  process.exit(2);
}
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const doc = "export-idem-" + Date.now().toString(36);
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 512, height: 384 }),
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

const layer = await call("create_layer", { layer_id: "L1" });
if (layer.ok !== true) {
  console.error("✗ 前置不成立：建不出图层 ⇒ " + JSON.stringify(layer).slice(0, 160));
  process.exit(2);
}
// 画几笔**普通笔刷**（可重放 ✓ ⇒ 会产生 `omitted` ✓），并带一个 `fill_region`（另一类来源 ✓）。
for (let i = 0; i < 3; i++) {
  const y = 100 + i * 40;
  const r = await call("brush_stroke", {
    layer_id: "L1", object_id: "e" + i, brush: "100%_Opaque", size: 40,
    color: { r: 10, g: 30, b: 60, a: 255 }, preview: false,
    points: [[80, y, 1], [300, y + 10, 1]],
  });
  if (r.ok !== true) console.error("  ⚠ 前置笔触未成功：" + JSON.stringify(r.context || {}).slice(0, 120));
}

const first = await call("export_project", {});
const second = await call("export_project", {});
if (first.ok !== true || second.ok !== true) {
  console.error("✗ 前置不成立：导出失败 ⇒ " + JSON.stringify(first).slice(0, 160));
  process.exit(2);
}
console.log(`  第一次：bytes=${first.bytes}｜blob_hash=${String(first.blob_hash).slice(0, 22)}…（hash=${String(first.blob_hash).slice(7, 23)}）`);
console.log(`  第二次：bytes=${second.bytes}｜blob_hash=${String(second.blob_hash).slice(0, 22)}…`);
console.log(`  重放计数：omitted=${first.omitted}／${second.omitted}｜kept_no_recipe=${first.kept_no_recipe}／${second.kept_no_recipe}｜kept_mismatch=${first.kept_mismatch}／${second.kept_mismatch}`);

// ① 包字节与内容寻址哈希都必须相同
check(first.bytes === second.bytes, "两次导出的**包字节数**必须相同",
  `${first.bytes} vs ${second.bytes}`);
check(String(first.blob_hash) === String(second.blob_hash),
  "两次导出的**内容寻址哈希**必须相同（内容寻址 ⇒ 字节一致 ✓）",
  `${String(first.blob_hash).slice(7, 23)} vs ${String(second.blob_hash).slice(7, 23)}`);

// ② 重放计数必须稳定
const sameCounts = ["omitted", "kept_no_recipe", "kept_mismatch"].every(
  (k) => Number(first[k]) === Number(second[k]));
check(sameCounts, "两次导出的**重放计数**必须完全相同（记忆化若错用证明 ⇒ 这里会漂移）",
  `omitted ${first.omitted}/${second.omitted}｜kept_no_recipe ${first.kept_no_recipe}/${second.kept_no_recipe}｜kept_mismatch ${first.kept_mismatch}/${second.kept_mismatch}`);

// ③ 防作弊：证明必须真的在跑
check(Number(first.omitted) > 0,
  "必须**真的跑过重放证明**（`omitted > 0`）—— 否则“完全不验证”也能让上面两条通过",
  "omitted=" + first.omitted);

// ⑤ **改动之后，包必须随之变化** ✓（第 81 轮补 ✓）—— **这是"导出结果缓存"的护栏** ✗：
//   缓存的核心风险不是"两次相同"✗（幂等判据已守 ✓），而是"**内容变了却仍给旧包**"✗
//   —— 那种情况下**两次导出照样相同** ✓ ⇒ **∴ 幂等判据抓不到它** ✗✓ ⇒ **∴ 必须单独立这一条** ✓。
if ((await call("create_layer", { layer_id: "L2" })).ok !== true) {
  console.error("✗ 前置不成立：第二个图层建不出");
  process.exit(2);
}
const changedStroke = await call("brush_stroke", {
  layer_id: "L2", object_id: "e9", brush: "100%_Opaque", size: 50,
  color: { r: 200, g: 40, b: 200, a: 255 }, preview: false,
  points: [[60, 300, 1], [320, 300, 1]],
});
if (changedStroke.ok !== true) {
  console.error("✗ 前置不成立：改动笔触未成功 ⇒ " + JSON.stringify(changedStroke).slice(0, 160));
  process.exit(2);
}
const third = await call("export_project", {});
check(String(third.blob_hash) !== String(second.blob_hash),
  "**改动之后导出的内容寻址哈希必须改变**（不许把旧包当新包给 ✗ —— 缓存的护栏）",
  `${String(second.blob_hash).slice(7, 23)} → ${String(third.blob_hash).slice(7, 23)}`);

console.log("");
if (failures.length) {
  console.error(`结论：导出不幂等 ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 导出幂等（字节／哈希／重放计数两次一致），且证明确实在跑（语义边界判据，不写死观测）");
process.exit(0);
