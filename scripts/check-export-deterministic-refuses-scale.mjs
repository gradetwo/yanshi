#!/usr/bin/env node
// **`deterministic: true` 必须**拒绝缩放**（第 1374 轮；实测撞出来的硬约束）
//
// **⇒ 为什么有这条** ✗：
//   `deterministic` 的语义是「**逐字节可复现**」⇒ 而缩放是**逐行浮点重采样** ⇒
//   那正是最可能的**非确定源** ⇒ 所以 `deterministic` **必须**拒绝 `width/height/max_edge` ✓
//   ⇒ 若哪天这个守卫被删掉 ⇒ **`deterministic` 就会悄悄不再是确定的** ✗
//   ⇒ 而那种「名字还叫 deterministic 但已经不确定」的退化，**没人会注意到** ✗
//
// **⇒ 断什么**（全是结构量 ⇒ 跨机器稳定 ✓）：
//   ① `deterministic:true` ＋ `max_edge` ⇒ **必须拒**（`ok=false`），且 detail 里提到「缩放」
//   ② `deterministic:true` 全幅 ⇒ **必须接受**（`ok=true`，`width` = 源宽）
//   ③ `deterministic:false` ＋ `max_edge` ⇒ **必须接受**，且 `width === max_edge`
//   ④ 同一次调用里**不许**既 `deterministic` 又缩放后仍返回 `ok=true`
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/check-export-deterministic-refuses-scale.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const doc = `det_${stamp}`;
let TOK = "";
const made = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }),
})).json();
TOK = made.token;
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

// ① det ＋ max_edge ⇒ 必须拒
const a = await post("export_png", { path: `d1_${stamp}.png`, deterministic: true, max_edge: 256 });
const aDetail = String((a.context || {}).detail || "");
if (a?.ok === false && /缩放/.test(aDetail)) ok(`① deterministic ＋ max_edge 被拒，且说清是缩放问题`);
else bad(`① deterministic ＋ max_edge **必须**被拒并提到「缩放」⇒ 实测 ok=${a?.ok}｜detail=${aDetail.slice(0, 70)}`);

// ② det 全幅 ⇒ 接受
const b = await post("export_png", { path: `d2_${stamp}.png`, deterministic: true });
if (b?.ok === true && b.width === 512) ok(`② deterministic 全幅被接受（${b.width}×${b.height}）`);
else bad(`② deterministic 全幅**必须**被接受 ⇒ 实测 ok=${b?.ok}｜width=${b?.width}`);

// ③ 非 det ＋ max_edge ⇒ 接受且真的缩小
const c = await post("export_png", { path: `d3_${stamp}.png`, deterministic: false, max_edge: 256 });
if (c?.ok === true && c.width === 256 && c.scaled === true) ok(`③ 非 deterministic ＋ max_edge 生效（${c.width}×${c.height}，scaled=true）`);
else bad(`③ 非 deterministic ＋ max_edge **必须**生效 ⇒ 实测 ok=${c?.ok}｜width=${c?.width}｜scaled=${c?.scaled}`);

// ④ 二次确认：det ＋ width/height 也必须拒（不是只拦 max_edge）
const d = await post("export_png", { path: `d4_${stamp}.png`, deterministic: true, width: 256, height: 256 });
if (d?.ok === false) ok("④ deterministic ＋ width/height 也被拒（两种写法一致）");
else bad(`④ deterministic ＋ width/height **必须**被拒 ⇒ 实测 ok=${d?.ok}`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（deterministic 必须拒绝缩放）`);
  process.exit(1);
}
console.log("\n  ✓ deterministic 的三条边界都成立（拒绝缩放／接受全幅／关掉后可缩放）");
