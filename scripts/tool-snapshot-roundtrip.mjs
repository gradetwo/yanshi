#!/usr/bin/env node
// **「快照 ⇒ 改动 ⇒ 还原 ⇒ 逐字节回到那时」判据** ✓（需求文档 P1-6 版本快照 ✓）。
// **先查了现状** ✓：这一族**已经全有**（`checkpoint` ✓ / `get_checkpoints` ✓ / `revert_to` ✓ /
// `restore_checkpoint` ✓ + `revert` ✓ + stash 三件 ✓）⇒ 所以本轮**不加工具** ✗，而是**验收那条性质** ✓。
// 判据（纯 RGBA 直通 ✓，行话与前面几条一致 ✓）：
//   ① 画一笔 ⇒ 拍照 A ✓；② `checkpoint{name}` ✓；③ 再画一笔（画面必须**变了** ✓，否则判据无效 ✗）；
//   ④ `restore_checkpoint{name}` ✓；⑤ 重新拍照 ⇒ **必须与 A 逐字节相同** ✓。
const base = process.argv[2];
const docId = process.argv[3] || "snap1";
if (!base) { console.error("用法: node scripts/tool-snapshot-roundtrip.mjs <base-url> [docId]"); process.exit(2); }
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
const shot = async () => {
  const raw = await call("render_region", { region: { x: 0, y: 0, w: 200, h: 150 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return null;
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
const stroke = (y) => call("brush_stroke", {
  layer_id: "layer_default", brush: "classic-brush",
  points: [[40, y, 0.5], [160, y, 0.5]], size: 16, color: { r: 40, g: 80, b: 160, a: 255 },
});
await call("create_layer", { layer_id: "layer_default", name: "l" });
const failures = [];
if (!(await stroke(50)).ok) { console.log("  ✗ 第一笔画不出来 ⇒ 判据无效"); process.exit(1); }
const before = await shot();
const checkpoint = await call("checkpoint", { name: "judge_snap" });
console.log("  checkpoint ⇒ ok=" + checkpoint.ok + "｜" + JSON.stringify(checkpoint).slice(0, 120));
if (checkpoint.ok !== true) { console.log("  ✗ 建不了检查点 ⇒ 判据无效 ⇒ " + JSON.stringify((checkpoint.context || {}).detail || checkpoint.error_code)); process.exit(1); }
if (!(await stroke(110)).ok) { console.log("  ✗ 第二笔画不出来 ⇒ 判据无效"); process.exit(1); }
const after = await shot();
if (!before || !after) { console.log("  ✗ 拿不到直通 RGBA ⇒ 判据无效"); process.exit(1); }
const changed = after.reduce((n, v, i) => (v !== before[i] ? n + 1 : n), 0);
console.log(`  第二笔之后：与 A 不同的字节 = ${changed}（总 ${after.length}）`);
// **自检** ✓：第二笔必须真的改变了画面 ✓（否则"还原成功"是平凡的 ✗）
if (changed === 0) { console.log("  ✗ 第二笔没改变任何字节 ⇒ 判据无效 ✗（比较会退化）"); process.exit(1); }
const checkpointId = checkpoint.checkpoint_id;
if (!checkpointId) { console.log("  ✗ checkpoint 没回 checkpoint_id ⇒ 判据无效 ⇒ 键=" + Object.keys(checkpoint).join(",")); process.exit(1); }
console.log("  用 checkpoint_id=" + checkpointId + " 还原 ✓（实测：这个工具**不收 name** ✗）");
const restored = await call("restore_checkpoint", { checkpoint_id: checkpointId });
console.log("  restore_checkpoint ⇒ ok=" + restored.ok + "｜" + JSON.stringify(restored).slice(0, 120));
const back = await shot();
if (!back) { console.log("  ✗ 还原后拿不到 RGBA ⇒ 判据无效"); process.exit(1); }
const same = back.equals(before);
console.log(`  还原后与 A 逐字节相同？ ${same ? "是 ✓" : "否 ✗"}`);
if (restored.ok !== true) failures.push("还原没成功 ⇒ " + JSON.stringify((restored.context || {}).detail || restored.error_code));
if (!same) {
  const diff = back.reduce((n, v, i) => (v !== before[i] ? n + 1 : n), 0);
  failures.push(`还原后仍有 ${diff} 个字节与快照前不同 ⇒ 没有真的回到那时 ✗`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 快照往返：改动确实发生，还原后逐字节回到快照那一刻");
