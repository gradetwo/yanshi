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

// ① **语义已变（第 1502 轮）**：二期补丁把 `type:"path"` **做成了正式支持** ✓
//   ⇒ 原来这条断言「写错 type 必须报错并提示 `d`」**已被产品超越** ✗
//   ⇒ 现在应断：**两种写法都能用**（`{"type":"path","d":…}` 与**只给 `d`**）✓
//   **⇒ 保留的历史理由**（仍然成立）：**报错不该把人挡在门外** ⇒ 现在不需要"提示"了，
//     因为**不需要绕过** ✓
const withType = await call("fit_background_field", {
  layer_id: "L", mask: { type: "path", d: "M 150,120 C 200,70 260,170 300,120 Z" } });
if (withType?.ok === true && (withType?.filled_pixels ?? 0) > 0) {
  ok(`① {\"type\":\"path\"} 现在**正式支持** ⇒ samples=${withType.samples}、filled=${withType.filled_pixels}`);
} else {
  bad(`① {\"type\":\"path\"} 应被支持 ⇒ 实测 ok=${withType?.ok}、detail=${String((withType?.context || {}).detail || "").slice(0, 90)}`);
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
console.log("\n  ✓ 两种写法都能用（`type:path` 与**只给 `d`**）");
