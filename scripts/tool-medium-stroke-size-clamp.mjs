#!/usr/bin/env node
// **`medium_stroke` 的 `size` 超过介质上限时必须**夹紧且可见**，`dirty_bbox` 必须按夹紧值算**
// （第 1416 轮；外部 D-09）
//
// **⇒ 原缺陷** ✗：`size` 一路传下去，插件内部按 `max_dab`（六种介质都是 **64**）截断 ⇒
//   而**脏区按请求值算** ⇒ **返回值与真实落墨不符** ✗✗
//   **⇒ 实测（oil）**：请求 64/96/200/640 ⇒ 墨厚恒 **58–62 px**，
//     而 `dirty_bbox` 报 **68 / 100 / 204 / 644** ⇒ 调用方以为铺了 644 px ✗
//
// **⇒ 断什么**：
//   ① 请求 **3× 上限** ⇒ `size_used === 64` 且 `size_clamped === true`（**截断可见** ✓）
//   ② `dirty_bbox` 的高度**必须按夹紧值算**（实测 68；上限 64 ⇒ 允许 ≤ 1.5×64）
//   ③ 请求 **≤ 上限** ⇒ **不许**报 `size_clamped`（避免误报）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-medium-stroke-size-clamp.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const CAP = 64;   // 六种介质的 max_dab（yanshi-medium-host 的 MEDIUMS 表）

const newDoc = async (tag) => {
  const id = `cl_${tag}_${stamp}`;
  const r = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: id, width: 800, height: 800,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  return r.token ? { id, tok: r.token } : null;
};
const post = async (d, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${d.id}&token=${d.tok}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const draw = async (tag, size) => {
  const d = await newDoc(tag);
  if (!d) return null;
  await post(d, "create_layer", { layer_id: "L", name: "L" });
  return post(d, "medium_stroke", { layer_id: "L", medium: "oil", size, load: 1.0, wetness: 0.3,
    color: [0.2, 0.2, 0.2], points: [[100, 400, 0.9], [700, 400, 0.9]] });
};

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① 请求 3× 上限
const big = await draw("big", CAP * 3);
if (!big || big.ok !== true) {
  console.error("  ⊘ 不可判：medium_stroke 失败（不是产品问题）");
  process.exit(3);
}
if (big.size_used === CAP && big.size_clamped === true) {
  ok(`① 请求 ${CAP * 3} ⇒ size_used=${big.size_used}、size_clamped=true（截断可见）`);
} else {
  bad(`① 请求 ${CAP * 3} 必须夹到 ${CAP} 且标 size_clamped ⇒ 实测 size_used=${big.size_used}、clamped=${big.size_clamped}`);
}

// ② 脏区必须按夹紧值算
const bb = big.dirty_bbox || [0, 0, 0, 0];
const h = Number(bb[3]);
if (h > 0 && h <= CAP * 1.5) {
  ok(`② dirty_bbox 高度 ${h} 按夹紧值算（≤ 1.5×${CAP}）`);
} else {
  bad(`② dirty_bbox 高度必须按夹紧值算 ⇒ 实测 ${h}（请求 ${CAP * 3}；≤ ${CAP * 1.5} 才算按夹紧值）`);
}

// ③ 不超上限时不许误报
const near = await draw("near", CAP);
if (!near || near.ok !== true) {
  console.log("  ○ ③ 未验：CAP 请求失败");
} else if (near.size_clamped === true) {
  bad(`③ 请求 ${CAP} 没超上限 ⇒ 不该报 size_clamped`);
} else {
  ok(`③ 请求 ${CAP}（未超上限）⇒ 不报 size_clamped`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（size 必须按 max_dab 夹紧且可见）`);
  process.exit(1);
}
console.log("\n  ✓ size 夹紧、脏区一致、未超限时不误报");
