#!/usr/bin/env node
// **`shape` 的 type 报错必须**提到路径怎么写****（第 1487 轮；外部 §232 的教训）
//
// **⇒ 外部实测** ✗：他们传 `{"type":"path","d":…}` ⇒ 报
//   「不支持的 shape type：「path」（**支持 rect/circle/ellipse/polygon**）」
//   ⇒ 那句话**暗示"不支持路径"** ⇒ 他们**找了五轮**才蒙到正解 `{"d": …}`（**不给 type**）✗✗
//   ⇒ 而**路径正是二期补丁的主打功能** ⇒ **报错把人挡在了门外** ✓
//
// **⇒ 本判据断两件事**（都是**结构量** ✓）：
//   ① 那条报错**必须**提到 `d`（拼法形如 `"d"`）⇒ 否则下一个人**还会误以为不支持** ✓
//   ② 正解 `{"d": …}` **必须真的能成** ⇒ 否则"提示"是在骗人 ✓
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/check-shape-error-hints-path.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const doc = `hint_${stamp}`;
const made = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 400, height: 300,
    background: { r: 255, g: 255, b: 255, a: 255 } }),
})).json();
if (!made.token) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
const call = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${made.token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
await call("create_layer", { layer_id: "L", name: "L" });
for (let k = 0; k < 10; k += 1) {
  await call("fill_region", { layer_id: "L",
    shape: { type: "rect", x: 10, y: 10 + k * 28, w: 380, h: 28 },
    color: [40 + k * 18, 40 + k * 18, 40 + k * 18], opacity: 1.0 });
}

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① **两种写法都必须真的对，而且**面积要对得上**（第 1503 轮；按二期整合包 README 的真值）
//   **⇒ 我原来的断言是"写错 type 必须报错并提示 d"** ⇒ 补丁把 `type:"path"` 做成了正式支持 ⇒ 语义变 ✓
//   **⇒ ⇒ 更严重的是我第二版的断言只断 `filled_pixels > 0`** ✗ —— README 指出：
//     **静默 rect 恰好给 10000** ⇒ 那样的断言**抓不到"其实是矩形"** ✗（**接近安慰剂**）
//   **⇒ ⇒ ⇒ 所以现在断**面积**：本样本（`M 150,120 C 200,70 260,170 300,120 Z`）的**解析真值 ≈ 1402**
//     （README 实测 1402；我实测 1403；**静默矩形会变 10000**）⇒ 容差 ±8% ✓
const AREA = 1402;
for (const [tag, mask] of [
  ["type:path+d", { type: "path", d: "M 150,120 C 200,70 260,170 300,120 Z" }],
  ["只给 d", { d: "M 150,120 C 200,70 260,170 300,120 Z" }],
]) {
  const r = await call("fit_background_field", { layer_id: "L", mask });
  const got = r?.filled_pixels ?? 0;
  if (r?.ok === true && Math.abs(got - AREA) <= AREA * 0.08) {
    ok(`① ${tag} ⇒ 面积 ${got}（解析 ${AREA}）✓ —— **不是**缺省矩形的 10000`);
  } else {
    bad(`① ${tag} ⇒ 面积 ${got}，应 ≈ ${AREA}（**10000 就意味着静默成了矩形**）`);
  }
}

// ② 正解必须真的能成
const right = await call("fit_background_field", {
  layer_id: "L", mask: { d: "M 150,120 C 200,70 260,170 300,120 Z" }, degree: 2 });
if (right?.ok === true && (right?.filled_pixels ?? 0) > 0) {
  ok(`② 正解 \`{"d": …}\` 真的能成 ⇒ samples=${right.samples}、filled=${right.filled_pixels}`);
} else {
  bad(`② 提示的正解必须能用 ⇒ 实测 ok=${right?.ok}、detail=${String((right?.context || {}).detail || "").slice(0, 80)}`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（路径掩膜两种写法）`);
  process.exit(1);
}
console.log("\n  ✓ 两种写法都能用，且面积与解析真值一致");
