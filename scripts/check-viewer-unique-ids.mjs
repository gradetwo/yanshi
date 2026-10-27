#!/usr/bin/env node
// **查看器 HTML 里的 `id` 必须全局唯一，且界面脚本引用的每个 `id` 必须真存在**（第五轮）
//
// **为什么需要这条静态判据** ✗：本轮把第四轮成果重接到上游新基线时 ✓
//   `git merge-file --union` 把**同一个「频率分离」卡片接了两份**（两侧在不同位置各加一次 ✓
//   而三方合并认为那是两处独立新增 ✗）⇒ 文件里出现两个 `id="cardFreqSep"` ✓
//   ⇒ `getElementById` **只回第一个** ⇒ 第二张卡的按钮**永远不会被接线** ✗
//   ⇒ 这就是本项目禁止的**「点了没反应」型静默失败** ✓。
//   **当时没有任何判据发现它** ✗（实测：`ls scripts/ | grep -i 'dup\|duplicate'` 命中 0 ✓）
//   ⇒ 浏览器判据抓不到 ✓：它查的是"功能能不能用" ✓ 而重复的按钮**不是任何一个功能** ✗。
//
// **它守两件事**：
//   ① **唯一性** ✓：`crates/yanshi-http/src/viewer.rs` 的 HTML 里同一个 `id=` 只许出现一次 ✓
//      （先剥掉 `<!-- -->` 注释块 ✓：注释里提到某个 id 不是第二个控件 ✓
//      本轮那两块注释就各含一次 `cardFreqSep` ✗ 不剥会把正确修复报成"还剩两个" ✓）；
//   ② **引用有主** ✓：`viewer-app.js` 里 `$("x")` / `getElementById("x")` 引用的每个 `x` ✓
//      要么在 HTML 里唯一存在 ✓ 要么由脚本**自己造出来**（`.id = "x"` ✓ `{ id: "x" ✓}` ✓
//      模板串里的 `id="x"` ✓）⇒ 否则那句 `$( )` 拿到 `null` ✓ 事件挂不上 ✗。
//
// **它不证明什么** ✗：不证明控件长得对、点下去行为对 ✓（那是 `browser-*.mjs` 的事 ✓）；
//   也不覆盖**运行时拼出来的动态 id**（`"thumb" + index` ✓ —— 那种引用本判据看不见 ✗）。
//
// **变异**（`--self-test` 用它们 ✓）：
//   ① HTML 里注进重复 id ⇒ 红；② 脚本引用一个不存在的 id ⇒ 红；
//   ③ 引用的 id 由脚本自己 `.id = ` 造出来 ⇒ **必须绿**（防假红 ✓）；
//   ④ 重复只出现在 `<!-- -->` 注释里 ⇒ **必须绿**（防假红 ✓）。
//
// 用法：node scripts/check-viewer-unique-ids.mjs [--self-test]
//   （CI 的 `run-criteria.sh` 默认分支会多传 `<base> <doc> <token>` ✓ 本判据**一律忽略** ✓）
// 退出码：0 = 唯一且引用有主｜1 = 有重复/悬空引用（产品回归）
//   ｜2 = 判据无法运行（模板形状与抽取逻辑脱节）｜3 = 读不到源码（不可判）
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const VIEWER_RS = join(ROOT, "crates/yanshi-http/src/viewer.rs");
const VIEWER_JS = join(ROOT, "crates/yanshi-http/assets/viewer-app.js");

const lineOf = (text, index) => text.slice(0, index).split("\n").length;

/** HTML 里出现的 `id=`（先剥 `<!-- -->`，注释不算控件）。 */
function htmlIds(htmlSource) {
  const visible = htmlSource.replace(/<!--[\s\S]*?-->/g, (m) => " ".repeat(m.length));
  const out = [];
  for (const m of visible.matchAll(/\sid="([^"]+)"/g)) {
    out.push({ id: m[1], line: lineOf(visible, m.index) });
  }
  return out;
}

/** 界面脚本自己造的 id（动态建的控件不算悬空引用）。 */
function jsCreatedIds(jsSource) {
  const out = new Set();
  for (const m of jsSource.matchAll(/\.id\s*=\s*["']([^"']+)["']/g)) out.add(m[1]);
  for (const m of jsSource.matchAll(/\{\s*id:\s*["']([^"']+)["']/g)) out.add(m[1]);
  for (const m of jsSource.matchAll(/\bid="([^"]+)"/g)) out.add(m[1]);
  return out;
}

function jsReferences(jsSource) {
  const out = new Map();
  for (const m of jsSource.matchAll(/\$\(\s*["']([A-Za-z][\w-]*)["']\s*\)/g)) {
    if (!out.has(m[1])) out.set(m[1], lineOf(jsSource, m.index));
  }
  for (const m of jsSource.matchAll(/getElementById\(\s*["']([A-Za-z][\w-]*)["']\s*\)/g)) {
    if (!out.has(m[1])) out.set(m[1], lineOf(jsSource, m.index));
  }
  return out;
}

/// 判据本身（**可对合成源码跑** ✓ —— `--self-test` 就靠这个 ✓）。
export function judge(htmlSource, jsSource) {
  const problems = [];
  const ids = htmlIds(htmlSource);
  if (ids.length < 50) {
    // 抽不到足够多的 id ⇒ 抽取逻辑与模板脱节 ⇒ 判据自己坏了（不给"0 条 ⇒ 绿" ✗）。
    problems.push(`⊘ 只从 HTML 里抽出 ${ids.length} 个 id（预期远多于此）⇒ 抽取逻辑与模板脱节`);
    return problems;
  }
  const byId = new Map();
  for (const { id, line } of ids) {
    if (!byId.has(id)) byId.set(id, []);
    byId.get(id).push(line);
  }
  for (const [id, lines] of byId) {
    if (lines.length > 1) {
      problems.push(`重复 id="${id}"（${lines.length} 次：viewer.rs 第 ${lines.join("、")} 行）⇒ getElementById 只回第一个 ⇒ 后一张卡的控件永远接不上线`);
    }
  }
  const unique = new Set([...byId.keys()]);
  const created = jsCreatedIds(jsSource);
  for (const [id, line] of jsReferences(jsSource)) {
    if (unique.has(id) || created.has(id)) continue;
    problems.push(`悬空引用：viewer-app.js:${line} 取 $("${id}") ⇒ HTML 里没有这个 id、脚本也没造它 ⇒ 拿回 null、事件挂不上`);
  }
  return problems;
}

const selfTest = process.argv.includes("--self-test");
if (selfTest) {
  const unexpected = process.argv.slice(2).filter((a) => a !== "--self-test");
  if (unexpected.length > 0) {
    console.error(`用法：node scripts/check-viewer-unique-ids.mjs [--self-test]（多余参数：${unexpected.join(" ")}）`);
    process.exit(2);
  }
  const html = (n) => Array.from({ length: n }, (_, i) => `<div id="cell${i}"></div>`).join("\n");
  const good = html(60) + '\n    <div class="card" id="cardFreqSep"><button id="freqSepRun"></button></div>\n    <!-- 注释里也提到 id="cardFreqSep"，不算第二个控件 -->\n  ';
  const js = `
    $("freqSepRun").addEventListener("click", run);
    const x = document.getElementById("cardFreqSep");
    const hint = document.createElement("div"); hint.id = "thumbNote"; $("thumbNote");
    const TOOLS = [{ id: "fillLayer", label: "填充" }]; $("fillLayer");
  `;
  const dup = good + '\n    <div class="card" id="cardFreqSep"><button id="freqSepRun2"></button></div>\n  ';
  const dangling = good + "\n    <button id=\"onlyOnce\"></button>\n  ";
  const jsDangling = js + '\n    $("suggestionReason");\n  ';
  const cases = [
    ["正例（唯一 + 引用有主，含注释里的同名与脚本自造的 id）", good, js, []],
    ["变异①HTML 注入重复 id", dup, js, ["重复 id"]],
    ["变异②脚本引用不存在的 id", good, jsDangling, ["悬空引用"]],
    ["变异③模板里根本没有这个 id 形状（抽取脱节）", "<p>没控件</p>", js, ["抽取逻辑与模板脱节"]],
  ];
  let bad = 0;
  for (const [name, htmlSource, jsSource, want] of cases) {
    const got = judge(htmlSource, jsSource);
    const ok = want.length === 0 ? got.length === 0 : want.every((w) => got.some((g) => g.includes(w)));
    if (!ok) {
      bad += 1;
      console.log(`  ✗ 自测：${name}｜期望命中 ${JSON.stringify(want)}｜实测 ${JSON.stringify(got)}`);
    } else {
      console.log(`  ✓ 自测：${name}`);
    }
  }
  process.exit(bad === 0 ? 0 : 1);
}

let htmlSource;
let jsSource;
try {
  htmlSource = readFileSync(VIEWER_RS, "utf8");
  jsSource = readFileSync(VIEWER_JS, "utf8");
} catch (error) {
  console.error(`  ⊘ 不可判：读不到源码（${error.message}）（不是产品问题）`);
  process.exit(3);
}
const problems = judge(htmlSource, jsSource);
const structural = problems.filter((p) => p.startsWith("⊘"));
if (structural.length > 0) {
  console.error("❌ 重复 id 判据无法运行：" + structural.join("；"));
  process.exit(2);
}
const ids = htmlIds(htmlSource);
console.log(`  HTML 里的 id：${ids.length} 个｜不同 id：${new Set(ids.map((i) => i.id)).size} 个｜界面脚本引用的 id：${jsReferences(jsSource).size} 个`);
if (problems.length > 0) {
  console.error(`✗ 查看器的控件标识不成立（${problems.length} 条）：`);
  for (const problem of problems.slice(0, 20)) console.error(`   - ${problem}`);
  if (problems.length > 20) console.error(`   … 另有 ${problems.length - 20} 条`);
  console.error("结论：id 是「按钮 ↔ 接线」的唯一桥梁 ✗ 重复或悬空都会让控件静默失灵 ⇒ 必须改回唯一 ✓。");
  process.exit(1);
}
console.log("  ✓ id 全局唯一（注释不算控件 ✓），且界面脚本引用的每个 id 都在 HTML 或脚本自建里 ✓");
