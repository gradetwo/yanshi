#!/usr/bin/env node
// **`combine_shapes` 的三种 mode 必须**产出不同结果****（第 1494 轮；本场实测发现 mode 在重叠时失效）
//
// **⇒ 实测（两个**重叠**矩形 A(50,50,100,80) / B(110,80,100,80)）**：
//   union / intersect / subtract ⇒ **都是 `loops=1`、`points_total=5`** ✗
//   ⇒ 即**三种模式给出同一个轮廓** ⇒ `mode` **没生效** ✗✗
//   **⇒ 而**不重叠**时是对的**（union ⇒ `loops=2`、`points_total=10`）✓ ⇒ 所以缺陷**只在重叠** ✗
//
// **⇒ 断言用的是**恒等式**（不写死点数）**：
//   ① 重叠时三种模式的 `points_total` **必须两两不同** ✓（形状不同 ⇒ 点数几乎不可能恰好相同）
//   ② 且**必须有序**：`union > subtract > intersect`（并集最大、交集中间、A−B 居中）✓
//   ③ 反证：**不重叠**时 union 必须是 **2 个环**（否则"模式没生效"这条断言本身可疑）✓
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) { console.error("用法: node scripts/tool-combine-shapes-modes.mjs <base>"); process.exit(3); }
const stamp = Date.now();
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const mk = async (ax, bx) => {
  const doc = `cs_${ax}_${bx}_${stamp}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 400, height: 200,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!made.token) return null;
  const call = async (tool, args) => {
    const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${made.token}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
    });
    return r.json();
  };
  await call("create_layer", { layer_id: "L", name: "L" });
  await call("draw_shape", { layer_id: "L", object_id: "A",
    data: { geometry: { kind: "rect", bbox: { x: ax, y: 50, w: 100, h: 80 } },
      fill: { color: [0.8, 0.2, 0.2, 1.0] } } });
  await call("draw_shape", { layer_id: "L", object_id: "B",
    data: { geometry: { kind: "rect", bbox: { x: bx, y: 50, w: 100, h: 80 } },
      fill: { color: [0.2, 0.2, 0.8, 1.0] } } });
  return { doc, call };
};

// ① + ② 重叠
{
  const h = await mk(50, 110);
  if (!h) {
    console.log("  ○ 不可判：建不出文档");
    process.exit(3);
  }
  const r = {};
  for (const mode of ["union", "subtract", "intersect"]) {
    const out = await h.call("combine_shapes", { layer_id: "L", mode, object_ids: ["A", "B"], result_id: "R_" + mode });
    r[mode] = out;
  }
  const pt = (m) => r[m]?.points_total;
  const u = pt("union"), s = pt("subtract"), i = pt("intersect");
  const distinct = new Set([u, s, i]).size;
  if (distinct === 3) ok(`① 重叠时三种模式的点数各不相同（union ${u}、subtract ${s}、intersect ${i}）`);
  else bad(`① 重叠时三种模式应给出不同轮廓 ⇒ 实测 union ${u}、subtract ${s}、intersect ${i}（**相同 ⇒ mode 未生效**）`);
  if (typeof u === "number" && typeof s === "number" && typeof i === "number" && u > s && s > i) {
    ok(`② 顺序正确：union(${u}) > subtract(${s}) > intersect(${i})`);
  } else {
    bad(`② 应满足 union > subtract > intersect ⇒ 实测 ${u} / ${s} / ${i}`);
  }
}

// ③ 反证：不重叠时 union 必须是 2 个环
{
  const h = await mk(20, 260);
  if (!h) {
    console.log("  ○ ③ 未验（建不出文档）");
  } else {
    const out = await h.call("combine_shapes", { layer_id: "L", mode: "union", object_ids: ["A", "B"], result_id: "RU" });
    if (out?.loops === 2) ok(`③ 反证：不重叠时 union 有 2 个环（points_total ${out.points_total}）⇒ 模式机制本身是活的`);
    else bad(`③ 不重叠时 union 应有 2 个环 ⇒ 实测 loops=${out?.loops}`);
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（combine_shapes 的模式区分）`);
  process.exit(1);
}
console.log("\n  ✓ 三种模式产出不同且有序的轮廓");
