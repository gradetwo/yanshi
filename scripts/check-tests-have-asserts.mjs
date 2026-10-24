#!/usr/bin/env node
// **测试文件里必须有断言**（第 856 轮）—— "注释不是断言"
//
// **⇒ 为什么要有这条** ✗：第 853 轮我提交了 `real_package_shrinks.rs` ✓
//   把三条判据写在 `//!` 文档注释里 ✓ 而**代码里一条 `assert!` 都没有** ✗
//   ⇒ 于是下一轮"变异不红"，我**查了一整轮**才明白问题不在产品、也不在变异 ✓
//   ⇒ 这份脚本把这件事**机检**掉 ✓：凡是**声称**有判据（文件里出现「判据」二字）的测试文件，
//     **必须**至少有一个 `assert!`／`assert_eq!`／`assert_ne!`／`panic!` ✓。
//
// **⇒ 判据（结构检查，如实说明）**：只证明"文件里有断言" ✓
//   —— 断言**读的是不是该读的量**，那条只能靠变异证明 ✓（这条脚本管不了 ✓）。
//   **⇒ 变异**：把某个测试文件的断言全删掉 ⇒ 红 ✓。
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const files = [];
for (const crate of readdirSync("crates")) {
  const dir = join("crates", crate, "tests");
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    continue;
  }
  for (const entry of entries) {
    if (entry.endsWith(".rs")) files.push(join(dir, entry));
  }
}

// **测量类文件豁免** ✓：`bench`／`probe`／`sweep` 是"量一张表"，**不是判据** ✓
//   ⇒ 它们的注释里出现「判据」二字是在**引用别人的判据** ✓
//   ⇒ 这条豁免**写死在这里**（而不是靠文件名约定心照不宣）✓
const MEASUREMENT = /(bench|probe|sweep)/;

const offenders = [];
let claiming = 0;
for (const file of files) {
  if (MEASUREMENT.test(file)) continue;
  const text = readFileSync(file, "utf8");
  if (!text.includes("判据")) continue;
  claiming += 1;
  const has_assert = /assert!|assert_eq!|assert_ne!|panic!/.test(text);
  if (!has_assert) offenders.push(file);
}

console.log(`  扫描 ${files.length} 个测试文件，其中**声称有判据**的 ${claiming} 个`);
for (const file of offenders) console.log(`  ✗ ${file} 提到「判据」但**没有任何断言**`);
if (offenders.length > 0) {
  console.error(`  ✗ ${offenders.length} 个测试文件的「判据」只写在注释里`);
  process.exit(1);
}
console.log("✓ 每个声称有判据的测试文件都至少有一条断言（结构检查：只证明「有断言」）");
