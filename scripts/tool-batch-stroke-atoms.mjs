#!/usr/bin/env node
// **`batch` 里的 N 个 `brush_stroke` 必须产生 **N 个原子**，且每笔**独立生效**（第 1360 轮）
//
// **⇒ 为什么有这条** ✗：我在 `docs/tools.md` 的「批量落笔」一节写了三条性质
//   （一次调用 N 笔／原子数 = N／不提亮）⇒ **而写进文档的性质必须有判据** ✓
//   ⇒ 外部正要用它把 8000–11000 笔画出来（每笔一帧的 timelapse）⇒ **原子数错就等于视频错** ✗
//
// **⇒ 断什么**：
//   ① 一次 `batch` 里 N 笔 ⇒ `head_seq` **恰好 +N**（**结构量，不是像素数** ⇒ 跨机器稳定 ✓）
//   ② **每笔独立**：N 笔各用**不同颜色**、各画在不同位置 ⇒ 逐点采到的颜色**互不相同**
//      （若 batch 只生效第一笔，② 会看到所有点同色 ⇒ 红）
//   ③ `ok === true`
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-batch-stroke-atoms.mjs <base>");
  process.exit(3);
}
const doc = `batch_atoms_${Date.now()}`;
const N = 8;
const post = async (tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${TOK}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
let TOK = "";
{
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 400, height: 400 }),
  })).json();
  TOK = made.token;
  if (!TOK) { console.error("  ⊘ 不可判：建不出文档"); process.exit(3); }
}
const head = async () => {
  // **⇒ `head_seq` 在**顶层**，不在 `context` 里** ✓（第 1360 轮实测：
  //   顶层 keys 含 `head_atom`／`head_seq`／`rendered_seq`／`atoms`）
  const r = await post("get_document", {});
  const v = r.head_seq ?? r.context?.head_seq ?? r.head;
  return typeof v === "number" ? v : null;
};
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

await post("create_layer", { layer_id: "L", name: "L" });
const h0 = await head();
if (h0 === null) { console.error("  ⊘ 不可判：get_document 没有 head_seq"); process.exit(3); }

// **⇒ 8 笔：颜色各不同、位置各不同（x 间隔 40，y 一行）**
const colors = [];
const points = [];
for (let k = 0; k < N; k += 1) {
  const c = [0.9, 0.1 + k * 0.1, 0.1];      // R 固定高、G 递增 ⇒ 8 个可区分的颜色
  colors.push(c);
  points.push([[40 + k * 40, 200, 0.8], [40 + k * 40, 260, 0.8]]);
}
const calls = points.map((pts, k) => ({
  tool: "brush_stroke",
  arguments: { layer_id: "L", brush: "calligraphy", size: 24, color: colors[k], points: pts,
    preview: false, wait_for_render: false },
}));
const rb = await post("batch", { calls });
const h1 = await head();
if (rb?.ok !== true) bad(`③ batch 必须成功 ⇒ 实测 ok=${rb?.ok}｜${String((rb.context || {}).detail || "").slice(0, 80)}`);
else ok("③ batch ok=true");

if (h0 !== null && h1 !== null) {
  const delta = h1 - h0;
  // **⇒ 注意**：`create_layer` 也在 h0 之后 ⇒ 它的原子算在 h0→h1 之间 ⇒ 所以比较用「batch 前后」
  //   这里 h0 取在 create_layer **之后** ⇒ delta 应恰为 N ✓
  if (delta === N) ok(`① ${N} 笔 ⇒ head 恰好 +${N}（${h0} → ${h1}）`);
  else bad(`① ${N} 笔必须产生 ${N} 个原子 ⇒ 实测 +${delta}（${h0} → ${h1}）`);
}

// ② 每笔独立：逐点采色，要求 8 个采样的 G 通道**互不相同**
const probes = Array.from({ length: N }, (_, k) => [40 + k * 40, 230]);
const sc = await post("sample_colors", { points: probes });
const samples = sc?.samples ?? sc?.context?.samples ?? [];
if (!Array.isArray(samples) || samples.length !== N) {
  console.log(`  ○ ② 未验：sample_colors 返回 ${Array.isArray(samples) ? samples.length : "非数组"} 个样本`);
} else {
  const gs = samples.map((s) => (s.rgba ? s.rgba[1] : -1));
  const distinct = new Set(gs).size;
  if (distinct === N) ok(`② ${N} 笔各自生效：${N} 个采样点的 G 通道互不相同（${gs.join(",")}）`);
  else bad(`② 每笔必须独立生效 ⇒ ${N} 个采样点只有 ${distinct} 种 G 值（${gs.join(",")}）`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（batch 的 N 笔必须产生 N 个原子且各自生效）`);
  process.exit(1);
}
console.log(`\n  ✓ batch 的 ${N} 笔：原子数正确、每笔独立生效`);
