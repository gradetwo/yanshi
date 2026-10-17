#!/usr/bin/env node
// **`region` 的形状在各工具上必须一致** ✓（第 1000+ 轮 ✓；Muse 的 P2-8 ✓）
//
// **∴ 修前 ✓**：`render_region`／`analyze_region` 声明 `Any`（收 `{x,y,w,h}` **或** `[x,y,w,h]` ✓），
//   而 `import_image`／`gradient_fill`／`texture_background` 声明 `Object`（**只收 `{…}`** ✗）
//   ⇒ **∴ 而解析层**早就统一**✓：它们都走 `parse_bbox()`（`tools.rs:1100` ✓），
//      **它本来就收两种**✓（**∴ 错误文案就是「region 必须是 {x,y,w,h} 或 [x,y,w,h]」✓**）
//   ⇒ **∴ 所以数组形状在**参数校验那一关**就被挡了 ✗，走不到 `parse_bbox` ✓。
//
// **∴ 这条判据怎么测 ✓（不需要知道其它参数的形状 ✓）**：
//   对每个带 `region` 的工具**只传** `{"region": [0,0,8,8]}` ✗
//   ⇒ **∴ 它必然因为**别的参数缺失**而失败 ✓（**∴ 那是正常 ✓**）
//   ⇒ **∴ 但失败原因里**不能出现 "region"**✗ ⇒ **∴ 出现了就说明 `region` 本身被拒 ✓ ⇒ **∴ 判据红 ✓**
import { readFileSync } from "node:fs";

const [base, doc, token] = process.argv.slice(2);
if (!base || !doc || !token) {
  console.error("用法: node scripts/tool-region-shape-parity.mjs <base> <doc> <token>");
  process.exit(3);
}
const src = readFileSync(new URL("../crates/yanshi-server/src/tools.rs", import.meta.url), "utf8");
const call = async (name, args) => {
  const url = `${base}/api/tools/${name}?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}`;
  const reply = await fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args) });
  return await reply.json();
};
// 从源码里找所有带 region 参数的工具名
// 按 `name: "` 切分：每一段只属于一个工具，段内出现 param!("region" 才算它有 region。
const tools = src
  .split(/name: "/)
  .slice(1)
  .map((chunk) => {
    const name = chunk.slice(0, chunk.indexOf('"'));
    return { name, hasRegion: chunk.includes('param!("region"') };
  })
  .filter((t) => t.hasRegion);
if (tools.length < 3) { console.log(`  ✗ 只从源码里找到 ${tools.length} 个带 region 的工具 ⇒ 判据跑不了`); process.exit(3); }

let bad = 0;
console.log(`  源码里带 region 的工具：${tools.length} 个 ✓`);
for (const { name } of tools) {
  const reply = await call(name, { region: [0, 0, 8, 8] });
  const detail = JSON.stringify(reply.context ?? reply).slice(0, 160);
  const rejectsRegion = /region/.test(detail) && /不接受|必须是|invalid type|expected/i.test(detail);
  console.log(`  ${rejectsRegion ? "✗" : "✓"} ${name}：${rejectsRegion ? "**拒绝数组形状** ✗" : "接受数组形状 ✓"}（${detail.slice(0, 80)}）`);
  if (rejectsRegion) bad += 1;
}
if (bad) { console.log(`  ✗ ${bad} 个工具不接受 [x,y,w,h] 形状的 region`); process.exit(1); }
console.log(`  ✓ ${tools.length} 个工具的 region 都接受数组形状（与解析层 parse_bbox 一致）`);
