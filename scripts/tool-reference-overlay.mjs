#!/usr/bin/env node
// **参考图叠加判据** ✓（需求文档 P2-9 ✓：临摹时半透明显示参考图，不用来回切换 ✓）。
// **先查了现状** ✓：查看器与服务端**都没有**这个功能 ✗ ⇒ 判据现在必然红 ✓（先写判据、先看红 ✓）。
//
// 判据只钉**最该守的性质** ✓（显示侧好不好看没法自动判 ✗，但"不许改动文档"必须判 ✓）：
//   ① `set_reference{blob_hash, opacity, position}` 被接受 ✓；
//   ② **设了参考图之后，文档的渲染结果必须逐字节不变** ✓（参考图是**叠在上面看的** ✓，
//      不是画进图层里的 ✗ —— 若它改了文档 ⇒ 用户"清掉参考图"也拿不回原画 ✓）；
//   ③ `clear_reference` 之后 ⇒ 仍与**设置之前**逐字节相同 ✓（可逆 ✓）。
const base = process.argv[2];
const docId = process.argv[3] || "ref1";
if (!base) { console.error("用法: node scripts/tool-reference-overlay.mjs <base-url> [docId]"); process.exit(2); }
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
  const raw = await call("render_region", { region: { x: 0, y: 0, width: 200, height: 150 }, raw: true, max_px: 4_000_000 });
  if (!raw.ok || !raw.raw_url) return null;
  return Buffer.from(await (await fetch(base + raw.raw_url)).arrayBuffer());
};
await call("create_layer", { layer_id: "layer_default", name: "l" });
const painted = await call("brush_stroke", {
  layer_id: "layer_default", brush: "classic-brush",
  points: [[40, 60, 0.5], [160, 60, 0.5]], size: 16, color: { r: 40, g: 80, b: 160, a: 255 },
});
if (painted.ok !== true) { console.log("  ✗ 先画一笔都失败 ⇒ 判据无效"); process.exit(1); }
const before = await shot();
// **一张最小的参考图**：用导出工具现造一个 blob 太重 ✗ ⇒ 直接给一个 `blob_hash` 形状即可 ✓
// （这一轮判的是"参考图不许动文档"✓，不是"参考图能显示"✗ —— 后者要查看器，判据管不了 ✓）。
const set = await call("set_reference", {
  blob_hash: "sha256:" + "0".repeat(64),
  opacity: 0.5,
  position: { x: 10, y: 10, w: 180, h: 130 },
});
if (set.ok !== true) {
  console.log("  set_reference ⇒ " + JSON.stringify((set.context || {}).detail || set.error_code || set).slice(0, 170));
  console.log("  ✗ 参考图功能不存在 ⇒ 判据红（预期 ✓）");
  process.exit(1);
}
const during = await shot();
const clear = await call("clear_reference", {});
const after = await shot();
if (!before || !during || !after) { console.log("  ✗ 拿不到直通 RGBA ⇒ 判据无效"); process.exit(1); }
const diff = (a, b) => a.reduce((n, v, i) => (v !== b[i] ? n + 1 : n), 0);
console.log(`  设参考图前/后差异字节 = ${diff(before, during)}（**必须为 0** ✓：参考图不许改文档）`);
console.log(`  清掉参考图后与最初差异字节 = ${diff(before, after)}（**必须为 0** ✓：可逆）`);
const failures = [];
if (!before.equals(during)) failures.push("设了参考图之后文档就变了 ⇒ 它是被**画进文档**了 ✗（应当是叠着看 ✓）");
if (!before.equals(after)) failures.push("清掉参考图后没回到原样 ⇒ 不可逆 ✗");
if (clear.ok !== true) failures.push("clear_reference 失败");
if (failures.length) { console.log("  ✗ " + failures.join("；")); process.exit(1); }
console.log("  ✓ 参考图：叠加期间与清除之后，文档都逐字节不受影响");
