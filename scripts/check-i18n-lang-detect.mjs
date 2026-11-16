#!/usr/bin/env node
// **浏览器语言自动检测的映射判据（静态）**。
//
// 为什么需要它 ✗：界面语言的读取链是「`?lang=` ＞ 已存选择 ＞ **浏览器语言检测** ＞ zh」✓
//（2026-11-17 Wang 令 ✓）。检测那一层若映射错（`pt-PT` 落到别家、未知语言不落 zh ✓），
// **每种语言的首访用户都会中招** ✗，而既有判据全都带着 `?lang=` 或已存选择进页面 ⇒ **永远测不到这一层** ✗。
// 本判据**只读源码** ✓：把 `viewer-app.js` 里的映射表 `UI_LANG_BY_BROWSER_PREFIX` 与纯函数
// `detectUILanguage` **原样抽出来真跑** ✓，逐项断言浏览器语言前缀的映射结果 ✓。
// 它断的是**关系**不是快照 ✓：映射表的键集必须与选择器的语言集 `UI_LANGS` **恒等** ✓
//（哪边多一种语言、另一边没跟上 ⇒ 当场红 ✓）。
//
// 退出码：0 通过 ✓；1 映射不对／没接线（**产品问题**）；2 抽不到表或函数（**判据与源码脱节**，不是产品问题）。
// 用法：node scripts/check-i18n-lang-detect.mjs        # 无参数、不需要服务端/浏览器
import { readFileSync } from "node:fs";

const VIEWER_JS = "crates/yanshi-http/assets/viewer-app.js";
const js = readFileSync(VIEWER_JS, "utf8");

const tableMatch = /const UI_LANG_BY_BROWSER_PREFIX = (\{[^}]*\});/.exec(js);
const fnMatch = /function detectUILanguage\(navLang\) \{[\s\S]*?\n\}/.exec(js);
const langsMatch = /const UI_LANGS = \[([\s\S]*?)\];/.exec(js);
if (!tableMatch || !fnMatch || !langsMatch) {
  console.error("❌ 语言检测判据无法运行：在 " + VIEWER_JS + " 里找不到 UI_LANG_BY_BROWSER_PREFIX / detectUILanguage / UI_LANGS（源码改名或拆分了 ⇒ 先更新本判据的锚点）");
  process.exit(2);
}
let detectUILanguage;
let table;
try {
  const built = new Function(`${tableMatch[0]}\n${fnMatch[0]}\nreturn { detectUILanguage, table: UI_LANG_BY_BROWSER_PREFIX };`)();
  detectUILanguage = built.detectUILanguage;
  table = built.table;
} catch (error) {
  console.error("❌ 语言检测判据无法运行：抽出的映射表/函数求值失败（" + error.message + "）");
  process.exit(2);
}
const selectorCodes = [...langsMatch[1].matchAll(/code: "([a-z]+)"/g)].map((hit) => hit[1]);

const problems = [];
// **恒等式**：映射表的键集 ≡ 选择器的语言集 ≡ 定案的 9 种 ✓（三边任意一边漂移都红 ✓）。
const EXPECTED = ["zh", "en", "fr", "es", "pt", "ja", "ko", "ru", "de"];
const sameSet = (a, b) => a.length === b.length && a.every((x) => b.includes(x));
if (!sameSet(selectorCodes, EXPECTED)) problems.push(`UI_LANGS 的语言集是 ${JSON.stringify(selectorCodes)}，与定案的 9 种 ${JSON.stringify(EXPECTED)} 不一致`);
if (!sameSet(Object.keys(table), selectorCodes)) problems.push(`映射表 UI_LANG_BY_BROWSER_PREFIX 的键集 ${JSON.stringify(Object.keys(table))} 与 UI_LANGS 的语言集 ${JSON.stringify(selectorCodes)} 不恒等（检测层与选择器不是同一套语言）`);
for (const [prefix, target] of Object.entries(table)) {
  if (target !== prefix) problems.push(`映射表里 ${JSON.stringify(prefix)} ⇒ ${JSON.stringify(target)}（主标签应落到同名语言；若某天真要别名映射，先改本判据并写明理由）`);
}
// **接线锚点**：函数存在 ≠ 被用 ✓ —— `uiLang` 的读取链必须真的调用它喂 `navigator.language` ✓
//（防"函数写了、链上没接"的安慰剂 ✓）。
if (!/detectUILanguage\([^)]*navigator\.language/.test(js)) {
  problems.push("接线缺失：viewer-app.js 里没有 detectUILanguage(... navigator.language ...) 的调用 ⇒ 检测函数是死代码，首访不会按浏览器语言选");
}
// **逐项映射**（每个受支持语言至少两个浏览器写法 + 大小写 + 未知落回 ✓）。
const CASES = [
  ["zh", "zh"], ["zh-CN", "zh"], ["zh-TW", "zh"], ["zh-Hans-CN", "zh"], ["zh-HK", "zh"],
  ["en", "en"], ["en-US", "en"], ["en-GB", "en"],
  ["fr", "fr"], ["fr-FR", "fr"], ["fr-CA", "fr"], ["fr-BE", "fr"],
  ["es", "es"], ["es-ES", "es"], ["es-MX", "es"], ["es-419", "es"],
  ["pt", "pt"], ["pt-BR", "pt"], ["pt-PT", "pt"],
  ["ja", "ja"], ["ja-JP", "ja"],
  ["ko", "ko"], ["ko-KR", "ko"],
  ["ru", "ru"], ["ru-RU", "ru"],
  ["de", "de"], ["de-DE", "de"], ["de-AT", "de"], ["de-CH", "de"],
  ["FR-FR", "fr"], ["EN-us", "en"], ["Pt-br", "pt"],
  ["it", "zh"], ["it-IT", "zh"], ["nl-NL", "zh"], ["sv-SE", "zh"], ["ar-EG", "zh"],
  ["", "zh"], [null, "zh"], [undefined, "zh"],
];
for (const [input, expected] of CASES) {
  const actual = detectUILanguage(input);
  if (actual !== expected) problems.push(`映射错：navigator.language=${JSON.stringify(input)} 应落 ${JSON.stringify(expected)}，实落 ${JSON.stringify(actual)}`);
}

console.log(`  选择器语言集：${selectorCodes.length} 种（${selectorCodes.join("/")}）｜映射表键：${Object.keys(table).length} 个｜映射用例：${CASES.length} 项`);
if (problems.length) {
  console.log(`  ✗ 浏览器语言检测映射不合格（${problems.length} 条）：`);
  for (const problem of problems.slice(0, 20)) console.log("     - " + problem);
  if (problems.length > 20) console.log(`     … 另有 ${problems.length - 20} 条`);
  process.exit(1);
}
console.log("  ✓ 每个受支持语言的浏览器前缀都映射到正确语言，未知前缀与空值落回 zh，且检测已接进 uiLang 读取链");
process.exit(0);
