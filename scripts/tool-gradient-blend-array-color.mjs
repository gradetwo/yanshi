#!/usr/bin/env node
// **P2-5：`gradient_blend` 的颜色三臂同口径**（期 2 块 2）
//
// **⇒ 缺陷**：`rgb_of` 只认对象与 `#rrggbb` ⇒ 数组色（全引擎通用写法）被拒
//   （`invalid_argument`）✗，且 `#RRGGBBAA` 与数组 4 元的 alpha 被静默丢掉 ✗。
//
// **⇒ 判据三件事**（同一对颜色，三种写法，各画一次）：
//   ① 数组臂成功（以前 `invalid_argument`）✓；
//   ② 三臂 `first_color`／`last_color` **逐字相同** ✓；
//   ③ `#RRGGBBAA` 臂的 `first_stroke.color.a` 是 128（alpha 进子笔，不静默丢）✓。
//
// **变异**：`rgba_of` 换回只认对象+`#rrggbb` ⇒ ① 红 ✓；alpha 改回只读对象 `a` ⇒ ③ 红 ✓。
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-gradient-blend-array-color.mjs <base>");
  process.exit(3);
}
const mkDoc = async (doc) => {
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 300, height: 200 }),
  })).json();
  if (!made.token) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
  return made.token;
};
const post = async (token, doc, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const arms = [
  // doc_id 只许 [A-Za-z0-9._-]（服务端 invalid_argument）⇒ 臂名中文只做显示，doc 用 ASCII slug。
  ["数组", "array", [235, 215, 200], [40, 80, 160]],
  ["对象", "object", { r: 235, g: 215, b: 200 }, { r: 40, g: 80, b: 160 }],
  ["hex", "hex", "#ebd7c8", "#2850a0"],
];
const results = [];
for (const [name, slug, from, to] of arms) {
  const doc = `gb_color_${slug}_${Date.now()}`;
  const token = await mkDoc(doc);
  await post(token, doc, "create_layer", { layer_id: "L", name: "L" });
  const result = await post(token, doc, "gradient_blend", {
    layer_id: "L",
    from: { x: 40, y: 100, color: from },
    to: { x: 260, y: 100, color: to },
    brush: "classic-brush", size: 24, steps: 5,
  });
  results.push([name, result]);
  if (result?.ok !== true) bad(`${name}臂失败 ⇒ ${JSON.stringify(result).slice(0, 200)}`);
  else ok(`① ${name}臂成功（strokes=${result.strokes}）`);
}
const [array, object, hex] = results.map(([, r]) => r);
if (array?.ok === true && object?.ok === true && hex?.ok === true) {
  if (array.first_color === object.first_color && array.first_color === hex.first_color
    && array.first_color === "#ebd7c8") {
    ok(`② 三臂 first_color 相同（${array.first_color}）`);
  } else {
    bad(`② 三臂 first_color 必须相同（数组 ${array.first_color}／对象 ${object.first_color}／hex ${hex.first_color}）`);
  }
  if (array.last_color === object.last_color && array.last_color === hex.last_color
    && array.last_color === "#2850a0") {
    ok(`② 三臂 last_color 相同（${array.last_color}）`);
  } else {
    bad(`② 三臂 last_color 必须相同（数组 ${array.last_color}／对象 ${object.last_color}／hex ${hex.last_color}）`);
  }
}

// ③ hex-8 alpha 进子笔
{
  const doc = `gb_alpha_${Date.now()}`;
  const token = await mkDoc(doc);
  await post(token, doc, "create_layer", { layer_id: "L", name: "L" });
  const result = await post(token, doc, "gradient_blend", {
    layer_id: "L",
    from: { x: 40, y: 100, color: "#ebd7c880" },
    to: { x: 260, y: 100, color: "#2850a0ff" },
    brush: "classic-brush", size: 24, steps: 5,
  });
  if (result?.ok !== true) bad(`③ hex-8 臂失败 ⇒ ${JSON.stringify(result).slice(0, 200)}`);
  else if (result.first_color === "#ebd7c8" && result.first_stroke?.color?.a === 128) {
    ok("③ #RRGGBBAA 的 alpha（0x80=128）进了子笔颜色，RGB 仍是 from 的色");
  } else {
    bad(`③ hex-8 臂：first_color 应为 #ebd7c8 且 first_stroke.color.a 应为 128，实测 ${result.first_color}／${result.first_stroke?.color?.a}`);
  }
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（gradient_blend 颜色口径）`);
  process.exit(1);
}
console.log("\n  ✓ gradient_blend：数组／对象／hex 三臂同色同结果，hex alpha 进子笔");
