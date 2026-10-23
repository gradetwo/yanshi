#!/usr/bin/env node
// **原子的来源必须可机械复核**：`kind` 是**存储形态**，`origin` 才是**来源** ✓（第 642 轮；外部 D5）
//
// **⇒ 他们的实测**：一幅 8K 画的 **39,354 条笔触原子 `kind` 全是 `"import_image"`** ✗
//   ⇒ 用它做"严禁导入参考图"的证明，会把"画出来的 38k 笔"误判成"导入了一张图" ✓。
//
// **⇒ 判据**：
//   ① 画一笔（`brush_stroke`）⇒ 那条原子的 `origin` 必须是 **`drawn`** ✓（**不管 `kind` 是什么** ✓）；
//   ② 建图层那类记账原子 ⇒ `origin` 必须是 **`system`** ✓；
//   ③ 每条原子都必须带 `origin` 字段（不许缺 ⇒ 缺失会让调用方又退回去看 `kind` ✗）。
//
// **变异**：把 `origin` 恒写成 `"imported"`（或删掉该字段）⇒ ① / ③ 红 ✓。
const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-atom-origin.mjs <base> <doc> <token>");
  process.exit(2);
}
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  return response.json();
};
const fail = (message) => {
  console.error(`  ✗ ${message}`);
  process.exit(1);
};
const layer = await call("create_layer", { layer_id: "layer_default", name: "l" });
if (layer.ok !== true && !String(layer.context?.detail ?? "").includes("已存在")) {
  fail(`create_layer ⇒ ${JSON.stringify(layer).slice(0, 120)}`);
}
const summary = await call("get_document", {});
const docW = Number(summary.width ?? 64);
const docH = Number(summary.height ?? 64);
const stroke = await call("brush_stroke", {
  layer_id: "layer_default",
  brush: "calligraphy",
  points: [
    [Math.round(docW * 0.2), Math.round(docH * 0.3), 0.5],
    [Math.round(docW * 0.8), Math.round(docH * 0.7), 0.6],
  ],
  size: Math.max(6, Math.round(Math.min(docW, docH) * 0.1)),
  color: [0.2, 0.4, 0.9, 1.0],
});
if (stroke.ok !== true) fail(`brush_stroke ⇒ ${JSON.stringify(stroke).slice(0, 160)}`);

const listed = await call("list_atoms", { limit: 50, offset: 0, include_non_stroke: true });
if (listed.ok !== true) fail(`list_atoms ⇒ ${JSON.stringify(listed).slice(0, 160)}`);
const atoms = listed.atoms ?? [];
if (atoms.length === 0) {
  console.log("  ⊘ 不可判：一条原子都没列出来");
  process.exit(3);
}
const missing = atoms.filter((atom) => typeof atom.origin !== "string");
if (missing.length > 0) {
  fail(`有 ${missing.length} 条原子缺 origin ⇒ 调用方只能退回去看 kind（这正是 D5 的病）`);
}
const drawn = atoms.filter((atom) => atom.origin === "drawn");
if (drawn.length === 0) {
  fail(`刚画的那一笔没有 origin=drawn ⇒ 来源不可机械复核（清单：${atoms.map((a) => `${a.kind}/${a.origin}`).slice(0, 6).join(", ")}）`);
}
const system = atoms.filter((atom) => atom.origin === "system");
if (system.length === 0) {
  fail(`建图层那类记账原子没有 origin=system ⇒ 来源分类不成立`);
}
const importedWithSource = drawn.filter((atom) => atom.kind === "import_image");
console.log(
  `✓ origin 可复核：${drawn.length} 条 drawn（其中 ${importedWithSource.length} 条 kind 仍是 import_image ⇒ 正是 D5 说的"存储形态≠来源"）、${system.length} 条 system、共 ${atoms.length} 条`,
);
