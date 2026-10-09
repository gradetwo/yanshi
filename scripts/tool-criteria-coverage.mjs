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
  // **∴ `pwa-sync-wasm.mjs` ✓**（第 610 轮 ✓）：**它**改产物**✗（**把 `crates/yanshi-wasm/pkg/`
  // 拷到 `web/wasm/` ✓**）⇒ **∴ 本就不该当判据 ✓**；**∴ 而"产物是否完整"由
  // `tool-pwa-assets.mjs` 判 ✓**（**它断言 wasm 存在且非空 ✓**）** ✓✓。
  "pwa-sync-wasm.mjs",
];
// **尚未接线** ✗：已知缺口 ✓ —— **每一条都必须写清"为什么还没接"** ✓（不许留空话 ✗）。
const NOT_WIRED = [
  // （当前为空 ✓）
];
// ⚠️ **已退休两条** ✓，两条的理由是**同一件事**：被测的那份产物已经不存在 ✓。
//   ① `wasm-brush-preview-parity.mjs`（第 797 轮 ✓）：读裸 `.wasm` 后**空 imports** instantiate ✗，
//      而 `default_preview_points` 今天**只在服务端** ✓、共享内核暴露的是另一套预览接口 ✓
//      ⇒ 它要守的"本地预览与服务端一致"**没有实现可测** ✓（与 `browser-brush-preview-local`
//      是同一个产品缺口：产品没做"本地出图" ✓）。
//   ② `wasm-brush-parity.mjs`（第 799 轮 ✓）：同样读裸 `.wasm` + 空 imports ✗，调
//      `yanshi_brush_alloc/paint/out_ptr` ✓ —— 那些符号**只存在于脚本自己里** ✓，
//      `crates/yanshi-brush-wasm` 已不存在 ✓。
//   ⇒ ⇒ **但要注意：它守的**性质**并没有失去守卫** ✓ —— 继任者 `kernel-brush-parity.mjs`
//      自己的头部就写着"与 `wasm-brush-parity.mjs` 的差别只有一个：那边用裸 wasm + C-ABI（门面），
//      这边用 bindgen 包（浏览器真正加载的那一份）" ✓，且它是**已接线、CI 绿**的 ✓。
//   ⇒ ⚠️ **我在这件事上错了两次** ✗，两次都因为**没读"另一个判据"的自述**：
//      先是**乐观**（把 `wasm-brush-parity` 接上线就说"该性质有守卫了" ✗ —— 它根本没跑 ✓），
//      后是**悲观**（发现它测退休门面就说"该性质今天没有判据在守" ✗ —— 继任者一直在跑 ✓）。
//      ⇒ **换判据/换实现时，"谁接手了这条性质"必须去**读那边的头部**** ✓，而不是从我手上的文件推断 ✓。

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
