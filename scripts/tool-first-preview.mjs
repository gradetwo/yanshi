#!/usr/bin/env node
// **★ "首次预览"必须如实标注 ✓ ★**（第 515 轮 ✓；目标第 3 条 ✓／外部报告 §5.1 ✓）
//
// **它判什么** ✓：**新文档的**第一次** `get_document` ⇒ `first_preview === true` ✓；
//   **其后**每次 ⇒ `false` ✓** —— **∴ 调用方据此区分"**慢一次**（**一次性生成缩略图 ＋ 预览 ✓**）
//   与"**慢每次**（**真回归 ✗**）"** ✓。
//
// **为什么重要** ✗：**外部报告**实测 **首次 1074 ms、其后 0.4～1.6 ms** ✓ ⇒
//   **∴ 若没有这个标注 ⇒ **∴ 调用方会把**一次性成本**误判成**性能回归**✗**（**撒谎的一种 ✓**）；
//   **∴ 而标注它**不需要改行为 ✓**，**只是把已经发生的事**如实说出来 ✓**（**符合"绝不撒谎 ✓"**）** ✓✓
//
// **判据** ✓：**① 首次 `true` ✓｜② 第二次 `false` ✓｜③ 第三次 `false` ✓**。
// **变异** ✗：**把标志恒置 `false`（**或恒 `true`**）⇒ **必红 ✓**。
//
// 用法：node scripts/tool-first-preview.mjs <base-url>
const base = process.argv[2];
if (!base) { console.error("用法: node scripts/tool-first-preview.mjs <base-url>"); process.exit(2); }
const post = async (path, body) => (await (await fetch(`${base}${path}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body ?? {}),
})).json());

const main = async () => {
  const doc = "fpx_" + Date.now().toString(36);
  const token = (await post("/api/documents", { doc_id: doc, width: 640, height: 480 })).token;
  const reads = [];
  for (let i = 0; i < 3; i++) {
    const r = await post(`/api/tools/get_document?doc=${doc}&token=${token}`, {});
    reads.push(r.first_preview);
  }
  console.log(`  连续 3 次 get_document ⇒ first_preview = ${JSON.stringify(reads)}`);
  const bad = [];
  if (reads[0] !== true) bad.push(`① 首次应为 true ✗（实测 ${JSON.stringify(reads[0])}）`);
  if (reads[1] !== false) bad.push(`② 第二次应为 false ✗（实测 ${JSON.stringify(reads[1])}）`);
  if (reads[2] !== false) bad.push(`③ 第三次应为 false ✗（实测 ${JSON.stringify(reads[2])}）`);
  if (bad.length) {
    console.error("❌ " + bad.join("｜") +
      "（变异：把标志恒置 false／true ⇒ 必红 ✓）");
    process.exit(1);
  }
  console.log("  ✓ 首次 true、其后 false ⇒ **∴ 一次性成本被如实标注 ✓**");
};
main().catch((e) => { console.error("❌ 运行失败：" + String(e)); process.exit(2); });
