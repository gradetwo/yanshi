#!/usr/bin/env node
// **`clear_all_caches` 必须把所有缓存都清掉（含 `above`）**（第 705 轮；外部审计 R-07）
//
// **⇒ 缺陷**：那个函数的注释自己写着「**新增缓存必须纳入这里**」✓，而它只清了
//   `cache`／`below`／`vector_raster` ⇒ **漏了 `above`** ✗ ⇒ 全量失效之后，上方层缓存
//   仍按**旧签名**命中 ⇒ **画面停在旧状态**（静默 ✗）。
//
// **⇒ 判据**：从 `render.rs` 里取出 `fn clear_all_caches` 的函数体 ✓，
//   断言它**逐个提到**这四样：`self.cache`／`self.below`／`self.vector_raster`／`self.above` ✓。
// **⇒ 变异**：删掉清 `above` 那三行 ⇒ 红 ✓。
//
// **⇒ 为什么用"读源码"而不是跑渲染** ✗：我试过用渲染去触发 `above` 复用 ✓，
//   但 `white_document` 那个夹具**根本不复用 above**（实测 `0 ⇒ 0`）✗
//   ⇒ 那样的判据是**空断言**（仓规禁止 ✓）⇒ 所以退回"结构检查"✓，
//   并在文案里**说清它只证明"这行代码在"** ✓（不谎称它验证了运行时行为 ✓）。
import { readFileSync } from "node:fs";

const src = readFileSync("crates/yanshi-render/src/render.rs", "utf8");
const at = src.indexOf("pub fn clear_all_caches(&mut self)");
if (at < 0) {
  console.error("  ⊘ 不可判：找不到 `clear_all_caches`（函数被改名了？这条判据要跟着改）");
  process.exit(3);
}
// 按大括号配平取函数体
let depth = 0;
let index = src.indexOf("{", at);
const start = index;
while (index < src.length) {
  if (src[index] === "{") depth += 1;
  else if (src[index] === "}") {
    depth -= 1;
    if (depth === 0) break;
  }
  index += 1;
}
const body = src.slice(start, index + 1);

const required = [
  ["self.cache", "瓦片缓存"],
  ["self.below", "下方层缓存"],
  ["self.vector_raster", "矢量单笔光栅缓存"],
  ["self.above", "**上方层缓存（本轮补的）**"],
];
const missing = required.filter(([needle]) => !body.includes(needle));
for (const [needle, what] of required) {
  const mark = body.includes(needle) ? "✓" : "✗";
  console.log(`  ${mark} 清 ${what}（\`${needle}\`）`);
}
if (missing.length > 0) {
  console.error(
    `  ✗ \`clear_all_caches\` 漏了 ${missing.length} 个缓存：${missing.map(([, what]) => what).join("、")} ⇒ 「全量失效」不成立（外部审计 R-07）`,
  );
  process.exit(1);
}
console.log("✓ `clear_all_caches` 四处缓存都清了（结构检查：只证明这行代码在，不声称验证运行时）");
