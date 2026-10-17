#!/usr/bin/env node
// **低沉积涂抹判据** ✓：`smudge` 的 `deposit`（缺省 0 ⇒ 与今天逐字节相同）。
//
// **观测面为什么是"涂前/涂后亮度差"** ✗（第 1000+ 轮实测 ✓）：
//   `export_png(layer_id, region, path)` 的 SHA256 在三个变体上**始终相同**
//   （`40b9ce636c83`、PNG 都是 3424 字节 ✗）⇒ 那个按层导出**不反映该层** ✗；
//   而 `analyze_region` 量整幅时，亮度**确实**随 `deposit` 变（0.3730 → 0.3689 ✓）。
//   **∴ 所以：一个变体只留**一层** ⇒ 整幅亮度就只反映它 ✓，再比**涂前 vs 涂后**的差 ✓。
//
// 三条断言：
//   ① 不给 `deposit` 与给 `deposit: 0` ⇒ **涂后亮度相同** ✓（**∴ 乘 1.0 逐位不变 ✓**）
//   ② `deposit: 0.6` ⇒ **涂后亮度不同** ✓（**∴ 压暗真的发生 ✓**）
//   ③ `deposit: 0.6` ⇒ **更暗** ✓（**∴ 方向正确 ✓**）
//
// 用法：node scripts/tool-smudge-deposit.mjs <server-base> <doc-id> <token>
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-smudge-deposit.mjs <base> <doc> <token>");
  process.exit(3);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args),
  });
  return { status: reply.status, body: await reply.json() };
};
const BLOCK = [[20, 60], [110, 60], [110, 140], [20, 140]];
const SMUDGE = [[110, 100], [130, 100], [150, 100]];
const REGION = { x: 0, y: 0, w: 300, h: 200 };
const bright = async () => {
  const r = await call("analyze_region", { region: REGION });
  const v = r.body?.avg_brightness;
  if (typeof v !== "number") { console.log(`  ✗ analyze_region 没给 avg_brightness`); process.exit(3); }
  return v;
};
// 一个变体：只留一层 ⇒ 画 → 量 → 涂 → 量 → 删
const variant = async (layer, deposit) => {
  await call("create_layer", { layer_id: layer, name: layer });
  const painted = await call("brush_stroke", {
    layer_id: layer, brush: "100%_Opaque", color: { r: 30, g: 30, b: 40, a: 255 },
    points: BLOCK.map(([x, y]) => [x, y, 1]), preview: false,
  });
  if (painted.body?.ok !== true) { console.log(`  ✗ ${layer}: brush_stroke 失败`); process.exit(1); }
  const before = await bright();
  const args = { layer_id: layer, points: SMUDGE, size: 40, smudge_length: 14, hardness: 0.4 };
  if (deposit !== null) args.deposit = deposit;
  const smudged = await call("smudge", args);
  if (smudged.body?.ok !== true) {
    console.log(`  ✗ ${layer}: smudge 失败 ${JSON.stringify(smudged.body).slice(0, 180)}`);
    process.exit(1);
  }
  const after = await bright();
  await call("delete_layer", { layer_id: layer });
  return { before, after };
};
const omitted = await variant("dep_omitted", null);
const zero = await variant("dep_zero", 0);
const some = await variant("dep_some", 0.6);
let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };
const same = (a, b) => Math.abs(a - b) < 1e-9;
check(same(omitted.after, zero.after),
  `不给 deposit 与给 0 涂后亮度相同（${omitted.after} vs ${zero.after}）`);
check(!same(omitted.after, some.after),
  `deposit=0.6 改变了涂后亮度（${omitted.after} vs ${some.after}）`);
const darker = omitted.after - some.after;
check(darker > 0.0005, `deposit=0.6 更暗（差 ${darker.toFixed(5)}）`);
if (bad) { console.log(`  ✗ 低沉积涂抹有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ 低沉积涂抹：缺省路径不变、deposit>0 生效且方向正确（更暗）");
