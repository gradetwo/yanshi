#!/usr/bin/env node
// **界面文案的双语覆盖率（静态）**判据。
//
// 为什么需要它 ✗：`browser-i18n.mjs` 能抓"英文模式下整页仍有中文" ✓，但它要**真浏览器 + 已构建的二进制** ✗
// ⇒ 没有 chromium 时它只打印「⊘ 跳过」✓ ⇒ **新加界面元素却忘了加词条**这件事可能完全没人守 ✗。
// 本判据**只读源码** ✓：把页面模板里"会变成文本节点"的中文串抽出来 ✓，
// 要求每一条都在 `I18N_EN_TEXT`（`crates/yanshi-http/assets/viewer-app.js` ✓）里有词条 ✓。
// 这正是新界面元素最容易漏的一步 ✓（本轮实测：文档列表表头、删除文档、诊断包、工程包共 10 条漏了 ✗）。
//
// **它覆盖什么、不覆盖什么**（说清楚，别让人以为它是全覆盖 ✗）：
//   * 覆盖 ✓：`viewer.rs` 的三个 HTML 片段（`PAGE_HEAD` / `PAGE_TAIL_A` / `PAGE_TAIL_B` ✓）里
//     **作为文本节点出现**的中文串 ✓ —— 按运行时**同一套 skip 规则**排除
//     （`#log` / `pre` / `code` / `textarea` / `script` / `style` / `[data-i18n=off]` ✓）。
//   * **不覆盖** ✗：
//       ① `title=` / `placeholder=` / `aria-label=` —— 那是 `I18N_EN_ATTR` 的事 ✓，
//          本判据**不检查**它 ⇒ 英文模式下 tooltip 仍可能是中文 ✓（要守它得另加一段 ✓）；
//       ② `viewer-app.js` 在**运行时拼出来**的文案（如 `"已删除文档 " + id` ✓）
//          与**模式翻译**（`m1…m7` ✓）—— 那些只有真浏览器能验 ✓；
//       ③ 翻译**质量** ✗ —— 本判据只看"有没有"，外加三条廉价的合法性（非空 / 不等于中文 / 不含汉字 ✓）。
//   ⇒ 所以它是 `browser-i18n.mjs` 的**静态影子** ✓，**不是**它的替代 ✓：两条都要留 ✓。
//
// 用法：node scripts/ui-i18n-coverage.mjs        # 无参数、不需要服务端/浏览器
import { readFileSync } from "node:fs";

const VIEWER_RS = "crates/yanshi-http/src/viewer.rs";
const VIEWER_JS = "crates/yanshi-http/assets/viewer-app.js";
const VIEWER_CSS = "crates/yanshi-http/assets/viewer.css";

// **抽原始字符串**：三个 HTML 片段必须都找得到 ✓ —— 找不到说明模板被改名/拆分了 ⇒
// **判据失效** ✗，此时要**报错退出** ✓，绝不能让"抽不到 ⇒ 0 条 ⇒ 绿"这种假绿发生 ✗。
function rawChunk(source, name) {
  const match = new RegExp(`const ${name}: &str = r##"([\\s\\S]*?)"##;`).exec(source);
  if (!match) {
    console.error("❌ 双语覆盖率判据无法运行：在 " + VIEWER_RS + " 里找不到 `const " + name + "` 的原始字符串");
    process.exit(2);
  }
  // 记下**内容在 viewer.rs 里的起始偏移** ✓ ⇒ 报错时能给**真的行号** ✓
  //（直接 `rs.indexOf(文案)` 会先撞到注释里同名的那一次 ✗ —— 实测「删除」指向了第 144 行的注释 ✓）。
  return { body: match[1], start: match.index + match[0].indexOf(match[1]) };
}

const rs = readFileSync(VIEWER_RS, "utf8");
// **按 `page_template()` 的同一顺序拼接** ✓（CSS / JS 各自在 `<style>` / `<script>` 里 ✓，
// 这样下面按标签剥离时 `<style>` 才有配对的 `</style>` ✓）。
const css = readFileSync(VIEWER_CSS, "utf8");
const js = readFileSync(VIEWER_JS, "utf8");
const head = rawChunk(rs, "PAGE_HEAD");
const tailA = rawChunk(rs, "PAGE_TAIL_A");
const tailB = rawChunk(rs, "PAGE_TAIL_B");
// **记住"拼接后每个字符来自 viewer.rs 的哪个偏移"** ✓ ⇒ 报错时给**真的行号** ✓
//（若直接对 viewer.rs 做 `indexOf(文案)`，会先撞到**注释里**的同名文案 ✗ ——
//  实测「删除」被报成第 144 行（注释 `**删除确认**` ✓），而真身在第 156 行 ✓）。
const parts = [
  { body: head.body, start: head.start },
  { body: css, start: null },
  { body: tailA.body, start: tailA.start },
  { body: js, start: null },
  { body: tailB.body, start: tailB.start },
];
const html = parts.map((part) => part.body).join("");
const fromRs = (index) => {
  let base = 0;
  for (const part of parts) {
    if (index < base + part.body.length) return part.start === null ? null : part.start + (index - base);
    base += part.body.length;
  }
  return null;
};

// **模拟运行时**：注释、`<script>`、`<style>` 整块先去掉 ✓（与 `i18nSkip()` 一致 ✓）；
// 同时维持「可见串 ⇒ viewer.rs 偏移」的映射 ✓。
const dropRanges = [];
for (const pattern of [/<!--[\s\S]*?-->/g, /<script[\s\S]*?<\/script>/g, /<style[\s\S]*?<\/style>/g]) {
  for (const hit of html.matchAll(pattern)) dropRanges.push([hit.index, hit.index + hit[0].length]);
}
dropRanges.sort((a, b) => a[0] - b[0]);
const merged = [];
for (const range of dropRanges) {
  const last = merged[merged.length - 1];
  if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]);
  else merged.push([range[0], range[1]]);
}
let visible = "";
const visibleFrom = [];
let drop = 0;
for (let i = 0; i < html.length; i += 1) {
  while (drop < merged.length && i >= merged[drop][1]) drop += 1;
  if (drop < merged.length && i >= merged[drop][0]) continue;
  visible += html[i];
  visibleFrom.push(fromRs(i));
}

const CJK = /[\u4e00-\u9fff]/;
// 与 `viewer-app.js` 的 `i18nSkip()` 同一套 ✓（`#log` 那个是按 id 认的 ✓）。
const SKIP_TAGS = new Set(["pre", "code", "textarea", "script", "style"]);

const candidates = new Map(); // 文本 ⇒ 行号（viewer.rs 里的出处 ✓，给人看的 ✓）
const stack = [];
const inSkip = () => stack.some((entry) => entry.skip);
const tagRe = /<(\/?)([a-zA-Z0-9]+)((?:[^>"']|"[^"]*"|'[^']*')*)>/g;
let cursor = 0;
let match;
while ((match = tagRe.exec(visible))) {
  const from = cursor;
  const text = visible.slice(from, match.index);
  cursor = tagRe.lastIndex;
  // **文本节点就是标签之间的那一整段** ✓ ⇒ 只 `trim()`、**不折叠内部空白** ✓
  //（运行时的 `i18nText()` 用的也是 `original.trim()` ✓ —— 折叠会让键与运行时不符 ✗）。
  if (!inSkip()) {
    const trimmed = text.trim();
    if (trimmed && CJK.test(trimmed) && !candidates.has(trimmed)) {
      const lead = text.length - text.trimStart().length;
      const at = visibleFrom[from + lead];
      candidates.set(trimmed, at === null || at === undefined ? null : rs.slice(0, at).split("\n").length);
    }
  }
  const closing = match[1] === "/";
  const tag = match[2].toLowerCase();
  const attrs = match[3] || "";
  if (closing) {
    for (let i = stack.length - 1; i >= 0; i -= 1) {
      if (stack[i].tag === tag) { stack.splice(i, 1); break; }
    }
    continue;
  }
  if (/\/\s*$/.test(attrs)) continue; // 自闭合标签不入栈 ✓
  const skip = SKIP_TAGS.has(tag) ||
    /data-i18n\s*=\s*["']off["']/.test(attrs) ||
    /id\s*=\s*["']log["']/.test(attrs);
  stack.push({ tag, skip });
}

// **抽词表**：只取 `const I18N_EN_TEXT = { … };` 那一段 ✓ —— 不能扫整个文件 ✗
//（后面还有 `I18N_EN_ATTR` ✓，混在一起会拿属性词条去解释文本节点 ✓）。
const textStart = js.indexOf("const I18N_EN_TEXT");
const attrStart = js.indexOf("const I18N_EN_ATTR");
if (textStart < 0 || attrStart < 0 || attrStart <= textStart) {
  console.error("❌ 双语覆盖率判据无法运行：在 " + VIEWER_JS + " 里找不到 I18N_EN_TEXT / I18N_EN_ATTR 两张表");
  process.exit(2);
}
const entryRe = /"((?:[^"\\]|\\.)*)"\s*:\s*"((?:[^"\\]|\\.)*)"/g;
const dict = new Map();
let entry;
while ((entry = entryRe.exec(js.slice(textStart, attrStart)))) {
  dict.set(entry[1].replace(/\\"/g, '"'), entry[2]);
}
if (dict.size === 0) {
  console.error("❌ 双语覆盖率判据无法运行：I18N_EN_TEXT 一条词条都没解析出来（表被改写了？）");
  process.exit(2);
}
// **自检**：抽不到足够多的模板文案 ⇒ 解析坏了 ⇒ 必须报错，而不是"0 条 ⇒ 绿" ✗。
if (candidates.size < 50) {
  console.error("❌ 双语覆盖率判据无法运行：只从模板里抽出 " + candidates.size + " 条中文文案（预期远多于此）⇒ 抽取逻辑已与模板脱节");
  process.exit(2);
}

// **运行时会整段改写掉的初值** ✓：模板里写着、但 `i18nWalk()` 永远看不到 ✓ ⇒ 不能要求它有词条 ✗
//（要求了就是**假红** ✗，会逼人往表里塞一条永远用不到的词条 ✓）。
// 每条都要有**锚点** ✓（证明它真的被 JS 改写 ✓）与**理由** ✓；锚点找不到 / 文案已消失 ⇒ **红** ✓（防清单腐化 ✓）。
const DYNAMIC = [
  {
    text: "撤销 0 / 重做 0",
    anchor: '$("undoDepth")',
    why: "footer 的初值：脚本段中部的 updateUndoStatus() 用 textContent 整段换成「可撤销 N 笔 / 可重做 N 笔」✓，末尾的 applyUILanguage() 只看得到换过之后的那份 ✓（动态那份由模式 m4 翻 ✓）",
  },
];

const problems = [];
for (const [text, line] of candidates) {
  if (dict.has(text)) continue;
  if (DYNAMIC.some((item) => item.text === text)) continue;
  problems.push(`漏词条：${line ? VIEWER_RS + ":" + line + " " : ""}${JSON.stringify(text)}`);
}
for (const item of DYNAMIC) {
  if (!candidates.has(item.text)) {
    problems.push(`清单过期：DYNAMIC 里的 ${JSON.stringify(item.text)} 在模板里已不存在 ⇒ 删掉那一条（否则清单会变成谎话 ✗）`);
  } else if (!js.includes(item.anchor)) {
    problems.push(`清单过期：DYNAMIC 里的 ${JSON.stringify(item.text)} 说理由是「${item.why}」，但锚点 ${JSON.stringify(item.anchor)} 在 ${VIEWER_JS} 里找不到 ✗`);
  }
}
// **词条合法性**（廉价的"占位翻译"守卫 ✓）：英文值不得为空、不得与中文相同、不得仍是汉字 ✓。
for (const [zh, en] of dict) {
  if (!en.trim()) problems.push(`词条非法：${JSON.stringify(zh)} 的英文是空的`);
  else if (en === zh) problems.push(`词条非法：${JSON.stringify(zh)} 的英文与中文一模一样（等于没翻 ✓）`);
  else if (CJK.test(en)) problems.push(`词条非法：${JSON.stringify(zh)} ⇒ ${JSON.stringify(en)} 里仍有汉字`);
}

console.log(`  模板里会进文本节点的中文文案：${candidates.size} 条｜I18N_EN_TEXT 词条：${dict.size} 条｜运行时会改写的初值：${DYNAMIC.length} 条`);
console.log("  ⚠️ 覆盖范围：只看文本节点 ✓；**不看** title/placeholder/aria-label（那是 I18N_EN_ATTR）✗，也不看 viewer-app.js 运行时拼的文案 ✗。");
if (problems.length) {
  console.log(`  ✗ 界面文案的双语覆盖率不合格（${problems.length} 条）：`);
  for (const problem of problems.slice(0, 20)) console.log("     - " + problem);
  if (problems.length > 20) console.log(`     … 另有 ${problems.length - 20} 条`);
  console.log("  结论：新加的界面文案必须在 I18N_EN_TEXT 里有词条 ✗ ⇒ 否则英文模式下会漏出中文 ✓。");
  process.exit(1);
}
console.log("  ✓ 模板里每一条会成为文本节点的中文文案都有英文词条，且词条不是占位");
