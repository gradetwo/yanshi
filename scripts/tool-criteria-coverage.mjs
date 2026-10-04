#!/usr/bin/env node
// **判据覆盖率守卫**（第 788 轮）：`scripts/*.mjs` 里的每个文件，要么**被 `run-criteria.sh` 枚举到** ✓，
// 要么出现在下面**两个显式清单**之一 ✓；否则本条**红** ✓。
//
// **为什么需要它** ✗：`run-criteria.sh` 是**按前缀**枚举的（`tool-*` / `browser-*` / 两条 parity ✓）⇒
// 任何不匹配前缀的判据会**静默地永不运行** ✗ —— 而「CI 全绿」看起来毫无异常 ✓。
// 第 787 轮实测：**74 个里 14 个从未运行过** ✗，其中含 `(A)⑥` 的核心判据 `wasm-brush-parity` ✗
//（它正是"同一条笔触在两种渲染模式下逐字节相同"那条 ✓）。
// ⇒ 本条把这件事变成**能被红出来的** ✓：**新增一个判据却忘了接线 ⇒ 立刻红** ✓。
//
// 判据（能红 ✓）：
//   ① 从 `run-criteria.sh` 的枚举行里解析出模式 ✓ ⇒ 用它判断每个文件名是否会被跑到 ✓；
//   ② 不在枚举里、也不在两个清单里 ⇒ **红并点名** ✓；
//   ③ **清单腐化也要红** ✓（清单里写了已不存在的文件 ⇒ 红 ✓）—— 否则清单会慢慢变成谎话 ✗。
//
// 用法：node scripts/tool-criteria-coverage.mjs [base-url]
//      （base-url 不使用 ✓，只为与 `tool-*.mjs` 的调用约定一致 ✓）

import { readFileSync, readdirSync } from "node:fs";

const RUNNER = "scripts/run-criteria.sh";

const runner = readFileSync(RUNNER, "utf8");
const enumLine = runner.split("\n").find((line) => line.includes("for script in $(ls"));
if (!enumLine) {
  console.error("❌ 覆盖率守卫无法运行：在 " + RUNNER + " 里找不到枚举行（它改过？）");
  process.exit(1);
}
const globs = [...enumLine.matchAll(/scripts\/([A-Za-z0-9*._-]+\.mjs)/g)].map((m) => m[1]);
if (globs.length < 2) {
  console.error("❌ 覆盖率守卫无法运行：只解析出 " + globs.length + " 个模式（疑解析失败）");
  process.exit(1);
}
const toRegExp = (glob) => new RegExp("^" + glob.replace(/[.+]/g, "\\$&").replace(/\*/g, ".*") + "$");
const patterns = globs.map(toRegExp);
const enumerated = (name) => patterns.some((re) => re.test(name));

// **生成器** ✓：它们改源码/产物，本就不该当判据 ✓。
const NOT_CRITERIA = [
  "generate-brush-previews.mjs",
  "generate-tool-examples.mjs",
  "make-samples.mjs",
];
// **尚未接线** ✗：已知缺口 ✓ —— **每一条都必须写清"为什么还没接"** ✓（不许留空话 ✗）。
const NOT_WIRED = [
  // ⚠️ **待改写** ✗（第 798 轮查明 ✓）：`wasm-brush-parity.mjs` ——
  //   它 `WebAssembly.instantiate(readFileSync(wasmPath), {})`（**空 imports** ✗）后调
  //   `yanshi_brush_alloc` / `yanshi_brush_paint` / `yanshi_brush_out_ptr` ✓ ——
  //   而这三个符号**只存在于判据脚本自己里** ✓、`crates/yanshi-brush-wasm` **已不存在** ✓
  //   ⇒ **它测的是已退休的第二份实现** ✗ ⇒ CI 实测
  //     `CompileError: expected magic word 00 61 73 6d, found 2f 2a 20 40`（把 `.js` 当 wasm ✓）。
  //   ⇒ **但不能就这么删掉它** ✗：它守的是 `(A)⑥` 的**头一条性质**（「同一条笔触在两种渲染模式下
  //     逐字节一致」✓）—— **这条性质今天**没有任何判据在守**** ✗ ✓。
  //   ⇒ **改写方案** ✓：改用它**今天真的存在**的那份门面 —— `crates/yanshi-wasm/pkg/` ✓
  //     （wasm-bindgen `--target web` ✓）；在 Node 侧 `import()` 那个 `.js` ✓，用 `initSync({ module })`
  //     喂入 `.wasm` 字节 ✓，再调共享内核的 `extend_preview_stroke` / `commit_preview` /
  //     `render_region_rgba` ✓（契约已在设计里读过 ✓），与服务端 `brush_stroke` 的对象 blob 逐字节比 ✓。
  "wasm-brush-parity.mjs",
];

const files = readdirSync("scripts").filter((name) => name.endsWith(".mjs")).sort();
const isEnumerated = files.filter(enumerated);
const isNotCriteria = NOT_CRITERIA.filter((name) => files.includes(name));
const isNotWired = NOT_WIRED.filter((name) => files.includes(name));

console.log("  scripts/*.mjs：共 " + files.length + " 个 ⇒ 被枚举 " + isEnumerated.length +
  "｜生成器 " + isNotCriteria.length + "｜尚未接线 " + isNotWired.length);

const unwired = files.filter(
  (name) => !enumerated(name) && !NOT_CRITERIA.includes(name) && !NOT_WIRED.includes(name)
);
if (unwired.length > 0) {
  console.error("❌ 有 " + unwired.length + " 个判据既没被 run-criteria.sh 枚举、也不在两个清单里");
  console.error("   ⇒ **它会静默地永不运行** ✗（而「CI 全绿」看不出任何异常）：");
  for (const name of unwired) console.error("     - " + name);
  console.error("   ⇒ 修法：接进 run-criteria.sh（可能要为它写一条 case 分支 ✓），");
  console.error("           或加进本文件的清单，并**写明为什么还没接** ✓。");
  process.exit(1);
}

const stale = [...NOT_CRITERIA, ...NOT_WIRED].filter((name) => !files.includes(name));
if (stale.length > 0) {
  console.error("❌ 清单腐化：这些名字在 scripts/ 里已不存在 ⇒ 清单在说已经不成立的话 ✗：");
  for (const name of stale) console.error("     - " + name);
  process.exit(1);
}

console.log("✓ 每个 scripts/*.mjs 都「要么被枚举、要么在清单里」⇒ 判据不会被静默跳过");
