#!/usr/bin/env node
// **越界参数必须报错，不许静默夹紧**（第 1419 轮；外部 D-12）
//
// **⇒ 原缺陷** ✗：
//   · `opacity: 5` ⇒ `.clamp(0.0, 1.0)` ⇒ **静默变成 1.0** ⇒ 调用方以为「5 很强」✗
//   · `points: [[x, y, 99]]` ⇒ `.clamp(0.0, 1.0)` ⇒ **静默变成 1.0** ⇒ 与 1.0 **完全一样** ✗
//   ⇒ **两者都是「非法输入看起来正常」** ⇒ 与 `read_canvas: "yes"` 同族 ✓
//
// **⇒ 断什么**：
//   ① `opacity: 5` ⇒ **ok=false**，且 detail 提到 `0..1`
//   ② `opacity: -1` ⇒ **ok=false**（负值同样）
//   ③ 压力 `99` ⇒ **ok=false**，且 detail 提到**第几个点**
//   ④ 正常值（`opacity 0.5` ＋ 压力 `0.8`）⇒ **ok=true**（不许误拒）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-brush-out-of-range-args.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const doc = `oor_${stamp}`;
const made = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 200, height: 200 }),
})).json();
const TOK = made.token;
if (!TOK) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
await post("create_layer", { layer_id: "L", name: "L" });

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };
const detailOf = (r) => String((r.context || {}).detail || r.detail || "");
const stroke = (extra) => post("brush_stroke", Object.assign({
  layer_id: "L", brush: "calligraphy", size: 20, color: [0, 0, 1],
  points: [[40, 60, 0.8], [160, 60, 0.8]], preview: false, wait_for_render: false,
}, extra));

// ① opacity 5
const o5 = await stroke({ opacity: 5 });
if (o5.ok === false && /0\.\.1/.test(detailOf(o5))) ok(`① opacity:5 被拒（${detailOf(o5).slice(0, 50)}…）`);
else bad(`① opacity:5 必须被拒并说清范围 ⇒ 实测 ok=${o5.ok}｜${detailOf(o5).slice(0, 60)}`);

// ② opacity -1
const on = await stroke({ opacity: -1 });
if (on.ok === false && /0\.\.1/.test(detailOf(on))) ok("② opacity:-1 被拒");
else bad(`② opacity:-1 必须被拒 ⇒ 实测 ok=${on.ok}`);

// ③ 压力 99
const p99 = await stroke({ points: [[40, 90, 99.0], [160, 90, 99.0]] });
if (p99.ok === false && /points\[0\]/.test(detailOf(p99))) {
  ok(`③ 压力 99 被拒且指出是第几个点（${detailOf(p99).slice(0, 50)}…）`);
} else {
  bad(`③ 压力 99 必须被拒并指出 points[0] ⇒ 实测 ok=${p99.ok}｜${detailOf(p99).slice(0, 60)}`);
}

// ④ 正常值不许被误拒
const good = await stroke({ opacity: 0.5 });
if (good.ok === true) ok("④ opacity 0.5 ＋ 压力 0.8 正常落笔（没误拒）");
else bad(`④ 正常值必须能落笔 ⇒ 实测 ok=${good.ok}｜${detailOf(good).slice(0, 60)}`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（越界参数必须报错，不许静默夹紧）`);
  process.exit(1);
}
console.log("\n  ✓ 越界参数被拒、错误指出位置、正常值不受影响");
