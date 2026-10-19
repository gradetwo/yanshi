#!/usr/bin/env node
// **判据自己的诚实**：用了 `fetch(` 的判据必须有 `process.exit(3)`（第 1082 轮）
//
// **∴ 依据（336 轮）**：判据面对「条件不满足」只有三种诚实回应
//   ① 通过（假称查过）② 跳过并说出来 ③ **不同退出码（exit(3)）**
//   ⇒ 而「默认通过」是第四种 ⇒ 看起来像查了 ⇒ **最危险** ✗
//
// **∴ 为什么需要这条**：无服务时 `fetch` **抛异常** ⇒ uncaught ⇒ node 退出码 **1**
//   ⇒ 于是「判据前提不成立」被报成「**产品失败**」✗（本机实测 1079 轮）
//
// **∴ 实测（1080 轮）**：191 个用 `fetch` 的判据里，**63 个缺 `process.exit(3)`**
//   ⇒ 所以这条判据**当前是红的** ⇒ 已登记 criteria-known-red.txt
//
// **∴ 为什么是静态检查**：真跑一遍（191 × 约 1 秒 ≈ 3 分钟）不适合作提交门；
//   而「用了 fetch 却没 exit(3)」是**必要不充分**条件 —— 它能抓住绝大多数，且**很快** ✓
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = new URL("../scripts/", import.meta.url).pathname;
const files = readdirSync(dir).filter((f) => /^(tool|mcp)-.*\.mjs$/.test(f)).sort();

const bad = [];
let withFetch = 0;
for (const f of files) {
  const text = readFileSync(join(dir, f), "utf8");
  if (!text.includes("fetch(")) continue;
  withFetch += 1;
  if (!text.includes("process.exit(3)")) bad.push(f);
}

console.log(`  用 fetch( 的判据：${withFetch} 个｜缺 process.exit(3)：${bad.length} 个`);
if (!withFetch) {
  console.error("  ⊘ 不可判：一个用 fetch( 的判据都没扫到（判据前提不成立）");
  process.exit(3);
}
// 只打印前 5 个，避免日志过长
for (const f of bad.slice(0, 5)) console.log(`    ✗ ${f}`);
if (bad.length > 5) console.log(`    …（还有 ${bad.length - 5} 个）`);
if (bad.length) {
  console.log(`  ✗ ${bad.length} 个判据无服务时会把「前提不成立」报成「产品失败」（应为 exit(3)）`);
  process.exit(1);
}
console.log(`  ✓ ${withFetch} 个用 fetch( 的判据都有 process.exit(3)（前提失败不会被误报成产品失败）`);
