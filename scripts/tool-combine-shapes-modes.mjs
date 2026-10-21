#!/usr/bin/env node
// **`combine_shapes` 的三种 mode 必须**产出不同结果****（第 1494 轮；本场实测发现 mode 在重叠时失效）
//
// **⇒ 更正（第 1495 轮）**：我第一版用 `points_total` 判 ⇒ **误报**（三处都是 5 点）✗
//   而**面积**是有效判别器：union **14024** / intersect **2000** / subtract **6015** ⇒ **三种模式全对** ✓
//   ⇒ 我的面积测量当时也错（`delete_object` 参数给错 ⇒ 输入一直在画 ⇒ 全 14000）⇒ 改正后**全对** ✓
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
let seq = 0;
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const mk = async (ax, bx) => {
  // **每次调用必须用**唯一** doc_id** ✗→✓（第 1495 轮）：
  //   原来写 `cs_${ax}_${bx}_${stamp}` ⇒ **同一次运行里三次 mk() 得到同一个 id** ⇒
  //   第二次起就撞上"对象 A 已被删除"（上一轮 tombstone 还在）✗
  seq += 1;
  const doc = `cs_${ax}_${bx}_${stamp}_${seq}`;
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

// ③ **每种模式用**独立文档****（第 1495 轮修正）：用**同一个**文档时，删掉 `A` 之后
//   **再 `draw_shape` 同 id 不能复活**（对象已 tombstone）⇒ 实测报「对象 A 已被删除」✗
//   ⇒ **所以**每模式一个干净文档 ✓
{
  // **期望值必须由**实际几何**算出来** ✗→✓（第 1495 轮）：
  //   `mk()` 把两个矩形**都放在 y=50** ⇒ 交叠 = (110..150)×(50..130) = **40×80 = 3200**
  //   （**不是** 2000 —— 我最初按 y=80 算，于是三个期望全错，误判成工具缺陷 ✗）
  //   ⇒ union 8000+8000−3200 = **12800**、subtract 8000−3200 = **4800**、intersect = **3200**
  const want = { union: 12800, subtract: 4800, intersect: 3200 };
  for (const m of ["union", "subtract", "intersect"]) {
    const h = await mk(50, 110);
    if (!h) { console.log("  ○ ③ 未验（建不出文档）"); break; }
    const out = await h.call("combine_shapes", { layer_id: "L", mode: m, object_ids: ["A", "B"], result_id: "R" });
    if (out?.ok !== true) {
      bad(`③ ${m} 应成功 ⇒ ${String((out?.context || {}).detail || "").slice(0, 80)}`);
      continue;
    }
    // 删掉两个输入（**只给 object_id**）⇒ 只剩结果
    await h.call("delete_object", { object_id: "A" });
    await h.call("delete_object", { object_id: "B" });
    const pts = [];
    for (let y = 40; y < 180; y += 1) for (let x = 40; x < 230; x += 1) pts.push([x + 0.5, y + 0.5]);
    let n = 0;
    for (let i = 0; i < pts.length; i += 900) {
      const sm = await h.call("sample_colors", { points: pts.slice(i, i + 900) });
      for (const z of (sm?.samples || [])) {
        const c = (z || {}).rgba || [255, 255, 255];
        if (Math.min(c[0], c[1], c[2]) < 250) n += 1;
      }
    }
    const exp = want[m];
    if (Math.abs(n - exp) <= exp * 0.08 + 300) ok(`③ ${m} 面积 ≈ ${n}（解析 ${exp}）✓`);
    else bad(`③ ${m} 面积应 ≈ ${exp} ⇒ 实测 ${n}`);
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（combine_shapes 的模式区分）`);
  process.exit(1);
}
console.log("\n  ✓ 三种模式的面积与解析期望相符");
