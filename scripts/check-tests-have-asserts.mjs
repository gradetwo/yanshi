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

// **⇒ 谓词抽成函数** ✅（第 1023 轮）：与 temp 判据同一做法 ⇒ **可自测** ✓
//   **⇒ 规则**：**剥掉注释后**仍要有 `assert!`／`assert_eq!`／`assert_ne!`／`panic!` ✓
function hasAssertionInCode(text) {
  const stripped = text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
  return /assert!|assert_eq!|assert_ne!|panic!/.test(stripped);
}

const offenders = [];
let claiming = 0;
for (const file of files) {
  if (MEASUREMENT.test(file)) continue;
  const text = readFileSync(file, "utf8");
  if (!text.includes("判据")) continue;
  claiming += 1;
  // **⇒ 注释里的断言不算** ✅（第 1006 轮，变异已证明 ✓）
  //   **⇒ 为什么** ✗：原版对**整份文本**正则 ⇒ 唯一那句 `assert!` 若写在 `//` 或 `//!` 里也照样通过 ✗
  //     （第 873 轮的形态正是"三条判据写在 `//!` 里、代码里一条都没有" ✓）
  //   **⇒ 变异（已验证）**：把 `ink_pixels.rs` 里含 assert 的 25 行全部注释掉 ⇒
  //     **剥离后剩余 assert! 计数 = 0** ⇒ 判据红：`✗ … 提到「判据」但没有任何断言` ✓
  //   **⇒ 注意**：验证时**必须用同一条谓词量残量** ✗ —— `grep -c assert` 会把**注释行**也数进去
  //     （第 1005 轮我就被它骗过一次 ✗）
  const has_assert = hasAssertionInCode(text);
  if (!has_assert) offenders.push(file);
}

console.log(`  扫描 ${files.length} 个测试文件，其中**声称有判据**的 ${claiming} 个`);
for (const file of offenders) console.log(`  ✗ ${file} 提到「判据」但**没有任何断言**`);
if (offenders.length > 0) {
  console.error(`  ✗ ${offenders.length} 个测试文件的「判据」只写在注释里`);
  process.exit(1);
}
console.log("✓ 每个声称有判据的测试文件都**在代码里**（不是注释里）至少有一条断言");
// **⇒ 自测** ✅（第 1023 轮）：三段小字符串证明这条判据**会红** ✓（不必动仓库 ✓）
//   **⇒ 变异**：让 `hasAssertionInCode` 直接返回 `true` ⇒ 中间那段变红 ✓
if (process.argv.includes("--self-test")) {
  const cases = [
    ["代码里有断言", "fn t() { assert!(x, \"y\"); }", true],
    ["断言只在注释里", "// assert!(x);\n/// assert!(y);\nfn t() {}", false],
    ["完全没有", "fn t() {}", false],
  ];
  let pass = 0;
  for (const [label, text, want] of cases) {
    const got = hasAssertionInCode(text);
    const okCase = got === want;
    console.log(`  ${okCase ? "✓" : "✗"} ${label}：expected ${want} got ${got}`);
    if (okCase) pass += 1;
  }
  console.log(`自测：${pass}/${cases.length} 通过`);
  process.exit(pass === cases.length ? 0 : 1);
}

