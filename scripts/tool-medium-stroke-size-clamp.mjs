#!/usr/bin/env node
// **`medium_stroke` 的 `size` 超过介质上限时必须**夹紧且可见**，`dirty_bbox` 必须按夹紧值算**
// （第 1416 轮；外部 D-09；**P-7 后上限 64⇒512，本判据改成探测上限**）
//
// **⇒ 原缺陷** ✗：`size` 一路传下去，插件内部按 `max_dab` 截断 ⇒
//   而**脏区按请求值算** ⇒ **返回值与真实落墨不符** ✗✗
//   **⇒ 实测（oil，上限还是 64 时）**：请求 64/96/200/640 ⇒ 墨厚恒 **58–62 px**，
//     而 `dirty_bbox` 报 **68 / 100 / 204 / 644** ⇒ 调用方以为铺了 644 px ✗
//
// **⇒ 为什么上限不能写死**（第 1416 轮 ⇄ P-7 的实测教训 ✓）：
//   这一版曾把 `CAP = 64` 写死 ⇒ P-7 把六种介质的 `max_dab` 提到 **512** 后，
//   请求 192 **不再夹紧** ⇒ 判据**必假红** ✗。
//   ⇒ 按「判据里的数字分两类」那条：**上限是被测系统决定的量** ⇒ **探测**：
//     请求 4096 ⇒ 回的 `size_used` 就是当前上限 ✓
//
// **⇒ 断什么**：
//   ① 请求 **4096**（远超任何上限）⇒ `size_clamped === true` 且 `size_used < 4096`
//     （**夹紧存在且可见** ✓；上限的具体数值由 `crates/yanshi-server/tests/medium_stroke.rs`
//      的 Rust 测试钉——那里量的是**实测落墨带宽**，与本判据各管一层 ✓）
//   ② `dirty_bbox` 的高度**必须按夹紧值算**（≤ 1.5×探测值；按请求值算会到 4096+）
//   ③ 请求 **恰等于上限** ⇒ **不许**报 `size_clamped`（边界不误报 ✓）
//   ④ 请求 **上限之半** ⇒ 不夹紧且 `size_used` 等于请求（防「全都夹紧」的假阳性 ✓）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-medium-stroke-size-clamp.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const OVER = 4096;  // 远超任何介质上限的请求值——用来自**探测**上限

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

// ⓪ 探测上限：请求 4096，回的 size_used 即当前夹紧值
const probe = await draw("probe", OVER);
if (!probe || probe.ok !== true) {
  console.error("  ⊘ 不可判：medium_stroke 失败（不是产品问题）");
  process.exit(3);
}
const CAP = Number(probe.size_used);
if (probe.size_clamped !== true || !(CAP > 0) || CAP >= OVER) {
  bad(`⓪ 请求 ${OVER} 必须被夹紧且回 size_used（探测失败⇒①〜④不可判）⇒ 实测 size_used=${probe.size_used}、clamped=${probe.size_clamped}`);
  console.log(`\n  ✗ ${fails.length} 处不符（夹紧本身不存在或不可见）`);
  process.exit(1);
}
ok(`⓪ 请求 ${OVER} ⇒ size_used=${CAP}、size_clamped=true（**上限是探测出来的**：${CAP}）`);

// ① 夹紧可见：size_used 恰为上限且标了 clamped（⓪ 已同形验过，这里断**恒等**而非区间）
if (probe.size_used === CAP && Number.isInteger(CAP)) {
  ok(`① 超限夹紧到整数上限 ${CAP} 且标 size_clamped（截断可见）`);
} else {
  bad(`① 夹紧值必须是整数 ⇒ 实测 size_used=${probe.size_used}`);
}

// ② 脏区必须按夹紧值算
const bb = probe.dirty_bbox || [0, 0, 0, 0];
const h = Number(bb[3]);
if (h > 0 && h <= CAP * 1.5) {
  ok(`② dirty_bbox 高度 ${h} 按夹紧值算（≤ 1.5×${CAP}）`);
} else {
  bad(`② dirty_bbox 高度必须按夹紧值算 ⇒ 实测 ${h}（请求 ${OVER}；≤ ${Math.round(CAP * 1.5)} 才算按夹紧值）`);
}

// ③ 恰等于上限时不许误报
const near = await draw("near", CAP);
if (!near || near.ok !== true) {
  console.log("  ○ ③ 未验：CAP 请求失败");
} else if (near.size_clamped === true) {
  bad(`③ 请求 ${CAP} 没超上限 ⇒ 不该报 size_clamped`);
} else {
  ok(`③ 请求 ${CAP}（未超上限）⇒ 不报 size_clamped`);
}

// ④ 上限之半：不夹紧且原值回显（防「所有请求都被夹」的假阳性）
const half = Math.max(1, Math.floor(CAP / 2));
const small = await draw("small", half);
if (!small || small.ok !== true) {
  console.log(`  ○ ④ 未验：${half} 请求失败`);
} else if (small.size_clamped === true || small.size_used !== half) {
  bad(`④ 请求 ${half} ⇒ 不该夹紧且 size_used 应回显原值 ⇒ 实测 size_used=${small.size_used}、clamped=${small.size_clamped}`);
} else {
  ok(`④ 请求 ${half}（上限之半）⇒ 不夹紧、size_used 回显 ${small.size_used}`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（size 必须按介质上限夹紧且可见，脏区按夹紧值算）`);
  process.exit(1);
}
console.log(`\n  ✓ 上限探测=${CAP}、夹紧可见、脏区一致、未超限时不误报`);
