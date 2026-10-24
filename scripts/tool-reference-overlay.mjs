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
if (!token) { console.error("判据自身前提不成立：拿不到 token（不是产品问题）"); process.exit(3); }
const call = async (tool, args) => (await fetch(`${base}/api/tools?doc=${docId}&token=${token}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ tool, arguments: args }),
})).json();
const shot = async () => {
  const raw = await call("render_region", { region: { x: 0, y: 0, w: 200, h: 150 }, raw: true, max_px: 4_000_000 });
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
// **一张最小的参考图**：先**真的放进存储** ✓（第 867 轮修）
//   **⇒ 原来的缺陷（判据自己）**：这里直接给 `sha256:000…0` 这个**形似而根本不存在的哈希** ✗
//     ⇒ 服务端的 `set_reference` **拒绝**它（"这个 blob 不在存储里 ⇒ 存下来也永远比不了" ✓ 合理 ✓）
//     ⇒ 于是判据报「参考图功能不存在」**误诊**了 3 个多月 ✓（功能一直在 ✓，缺的是**一个真 blob** ✓）
//   **⇒ 改法**：用 `put_blob` 造一个 **2×2 的裸 RGBA** ✓（64 字节 ✓），拿它返回的 `blob_hash` ✓
const tiny = Buffer.alloc(16, 200).toString("base64"); // 2×2 RGBA，每通道 200 ⇒ 合法 PNG 不重要，裸字节即可
const put = await call("put_blob", {
  data_base64: tiny,
  mime_type: "image/x-yanshi-raw",
});
if (put && put.ok !== true) {
  console.log("  put_blob ⇒ " + JSON.stringify((put.context || {}).detail || put.error_code || put).slice(0, 170));
  console.log("  ⊘ 造不出 blob ⇒ 判据前提不成立（不是产品问题）");
  process.exit(3);
}
const refHash = put.blob_hash;
if (typeof refHash !== "string" || !refHash.startsWith("sha256:")) {
  console.log("  put_blob 没返回 blob_hash ⇒ 判据前提不成立：" + JSON.stringify(put).slice(0, 170));
  process.exit(3);
}
const set = await call("set_reference", {
  blob_hash: refHash,
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
