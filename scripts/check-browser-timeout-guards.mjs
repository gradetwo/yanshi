#!/usr/bin/env node
// **每个 browser-* 判据都要有**总超时守卫**（L-3 收尾）**
//
// **⇒ 为什么** ✗：这些判据要连 CDP 浏览器 ✓ ⇒ 浏览器/页面起不来时它们可能**一直等**
//   ⇒ 运行器的外部 `timeout` 到点会把它们**当失败（EXIT=124）** ✗
//   ⇒ 而本仓约定是「判据自己跑不了 ⇒ `exit(3)` 并说**不是产品问题**」✓
//   ⇒ **∴ 有守卫** ＝ 到点**退回 3**（本次没验）＋ **不是冒充产品失败** ✓。
//
// **⇒ 判什么**（开集，新判据漏守卫会当场红 ✓）：
//   ① 每个 `scripts/browser-*.mjs` 里都有**一个顶层** `const X = setTimeout(() => { … })`；
//   ② 它的回调里**既有** `process.exit(3)` **又有**「不是产品」这句自述
//      （只说 exit(3) 而不解释 ⇒ 读者会以为产品失败 ✗）。
//
// **⇒ 不判什么**：不判时长（各条预算不同 ✓）；不证明浏览器真的会被杀掉 ——
//   那是 `run-criteria.sh` 的外部 `timeout` 的事 ✓，这里只守「有自述、退 3」这个结构。
//
// **⇒ 变异**（`--self-test` 用合成源码验 ✓）：
//   ① 删掉守卫 ⇒ 红；② 回调里没有「不是产品」⇒ 红；③ 退 1 而不是 3 ⇒ 红。
//
// 用法：node scripts/check-browser-timeout-guards.mjs [--self-test]
// 退出码：0 = 每个 browser-* 都有守卫｜1 = 有判据缺守卫/守卫不自述｜3 = 读不到 scripts/（不可判）
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/// 判据本身（**可对合成源码跑** ✓ —— `--self-test` 就靠这个 ✓）。
export function judge(files) {
  const problems = [];
  const missing = [];
  const silent = [];
  const wrongCode = [];
  // 顶层 `const X = setTimeout(() => {`，再在它后面一段窗口里找退 3 与自述。
  // **不按 `},` 截回调体** ✗：回调里可能嵌对象/调用（如 `fetch(..., {headers: {...}, body})`）
  // ⇒ 第一次 `},` 落在那里面 ⇒ 截短了就看漏后面的 `process.exit(3)`（实测：browser-reference-decode ✓）。
  const WATCHDOG = /(?:^|\n)(?:const|let|var)\s+[A-Za-z_$][\w$]*\s*=\s*setTimeout\s*\(\s*\(\s*\)\s*=>\s*\{/;
  for (const { name, source } of files) {
    const match = source.match(WATCHDOG);
    if (!match) {
      missing.push(name);
      continue;
    }
    const body = source.slice(match.index, match.index + 1200);
    if (!body.includes("process.exit(3)")) wrongCode.push(name);
    if (!body.includes("不是产品")) silent.push(name);
  }
  if (missing.length > 0) problems.push("缺总超时守卫：" + missing.join("、"));
  if (silent.length > 0) problems.push("守卫退 3 却没自述「不是产品」：" + silent.join("、"));
  if (wrongCode.length > 0) problems.push("守卫没退 3（会冒充产品失败）：" + wrongCode.join("、"));
  return problems;
}

const selfTest = process.argv.includes("--self-test");
if (selfTest) {
  const good = `
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 退出码 3");
  process.exit(3);
}, 90000);
`;
  const relaxed = good.replace("\n__judgeTimeout", "");
  const noGuard = "const x = 1;\n";
  const silentGuard = `
const guard = setTimeout(() => {
  process.exit(3);
}, 90000);
`;
  const wrongExit = `
const guard = setTimeout(() => {
  console.error("不是产品问题吗");
  process.exit(1);
}, 90000);
`;
  const cases = [
    ["正例（守卫在、退 3、有自述）", [{ name: "ok.mjs", source: good }], []],
    ["正例（任意变量名、无 unref）", [{ name: "relaxed.mjs", source: relaxed }], []],
    ["变异①没有守卫", [{ name: "none.mjs", source: noGuard }], ["缺总超时守卫"]],
    ["变异②退 3 但不说「不是产品」", [{ name: "silent.mjs", source: silentGuard }], ["没自述"]],
    ["变异③退 1 冒充产品失败", [{ name: "wrong.mjs", source: wrongExit }], ["没退 3"]],
  ];
  let bad = 0;
  for (const [name, files, want] of cases) {
    const got = judge(files);
    const ok = want.length === 0 ? got.length === 0 : want.every((w) => got.some((g) => g.includes(w)));
    if (!ok) {
      bad += 1;
      console.log(`  ✗ 自测：${name}｜期望 ${JSON.stringify(want)}｜实测 ${JSON.stringify(got)}`);
    } else {
      console.log(`  ✓ 自测：${name}`);
    }
  }
  process.exit(bad === 0 ? 0 : 1);
}

let names;
try {
  names = readdirSync(join(ROOT, "scripts"))
    .filter((f) => /^browser-.*\.mjs$/.test(f))
    .sort();
} catch (error) {
  console.error(`  ⊘ 不可判：读不到 scripts/（${error.message}）（不是产品问题）`);
  process.exit(3);
}
// 集合为空 ⇒ 判据没在验任何东西（四层齐之「输入能触发」）。
if (names.length === 0) {
  console.error("  ⊘ 不可判：scripts/ 下没有 browser-*.mjs（不是产品问题）");
  process.exit(3);
}
const files = names.map((name) => ({
  name,
  source: readFileSync(join(ROOT, "scripts", name), "utf8"),
}));
const problems = judge(files);
if (problems.length > 0) {
  console.error(`✗ 浏览器判据的总超时守卫不齐（${problems.length} 条）：`);
  for (const problem of problems) console.error(`   - ${problem}`);
  process.exit(1);
}
console.log(`✓ ${names.length} 条 browser-* 都有总超时守卫（退 3 且自述不是产品问题）`);
