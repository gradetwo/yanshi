#!/usr/bin/env node
// **界面文案的多语言覆盖率（静态）**判据（原为双语 ✓ 2026-11-17 扩到 8 个目标语言 ✓）。
//
// 为什么需要它 ✗：`browser-i18n.mjs` 能抓"英文模式下整页仍有中文" ✓，但它要**真浏览器 + 已构建的二进制** ✗
// ⇒ 没有 chromium 时它只打印「⊘ 跳过」✓ ⇒ **新加界面元素却忘了加词条**这件事可能完全没人守 ✗。
// 本判据**只读源码** ✓：把页面模板里"会变成文本节点"的中文串抽出来 ✓，
// 要求每一条都在 `I18N_EN_TEXT`（`crates/yanshi-http/assets/i18n.js` ✓，NEW-H1 瘦身起从主脚本挪出 ✓）里有词条 ✓。
// 这正是新界面元素最容易漏的一步 ✓（本轮实测：文档列表表头、删除文档、诊断包、工程包共 10 条漏了 ✗）。
//
// **它覆盖什么、不覆盖什么**（说清楚，别让人以为它是全覆盖 ✗）：
//   * 覆盖 ✓：`viewer.rs` 的三个 HTML 片段（`PAGE_HEAD` / `PAGE_TAIL_A` / `PAGE_TAIL_B` ✓）里
//     **作为文本节点出现**的中文串 ✓ —— 按运行时**同一套 skip 规则**排除
//     （`#log` / `pre` / `code` / `textarea` / `script` / `style` / `[data-i18n=off]` ✓）。
//   * **不覆盖** ✗：
//       ① ~~`title=` / `placeholder=` / `aria-label=`~~ —— **现已覆盖** ✓（见下面「属性文案也要有词条」那一段 ✓：
//          原先这一段明说不查属性 ✓ ⇒ 模板里 41 条中文 tooltip 一条词条都没有 ✓ 而判据是绿的 ✗）；
//       ② `viewer-app.js` 在**运行时拼出来**的文案（如 `"已删除文档 " + id` ✓）
//          与**模式翻译**（`m1…m7` ✓）—— 那些只有真浏览器能验 ✓；
//       ③ 翻译**质量** ✗ —— 本判据只看"有没有"，外加三条廉价的合法性（非空 / 不等于中文 / 不含汉字 ✓）。
//   ⇒ 所以它是 `browser-i18n.mjs` 的**静态影子** ✓，**不是**它的替代 ✓：两条都要留 ✓。
//
// 用法：node scripts/ui-i18n-coverage.mjs        # 无参数、不需要服务端/浏览器
//
// **2026-11-17 扩展** ✓：目标语言从英文 1 种扩到 8 种（再加 fr/es/pt/ja/ko/ru/de ✓）⇒
// 文件末段对其余 7 种逐一对账（键集与 EN 精确相等、零空译、白名单双向守 ✓）；
// 上面各段的英文口径一个字没动 ✓（运行时覆盖判据 check-i18n-runtime-coverage 仍以英文表与 m 模式为锚 ✓）。
import { readFileSync } from "node:fs";

const VIEWER_RS = "crates/yanshi-http/src/viewer.rs";
const VIEWER_JS = "crates/yanshi-http/assets/viewer-app.js";
// **NEW-H1 瘦身**：三张词表挪成独立资产 ⇒ **抽表读这份**，锚点/运行时逻辑仍读 `VIEWER_JS` ✓
//（**同一事实只写一处 ✗ 表若被挪回主脚本，本判据当场报"找不到表" ✓ 不会假绿**）。
const I18N_JS = "crates/yanshi-http/assets/i18n.js";
// （`VIEWER_CSS` 不再被读本判据 ⇒ NEW-H1 第二步 CSS 外链后它不进页面字节流 ✓ 词表/样式各归资产 ✓）

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
// **按 `page_template()` 的同一顺序拼接** ✓（**NEW-H1 瘦身后 CSS 与词表都外链 ✗ 不再进页面字节流**
// ⇒ 这里也不拼它们 ✓；`<style>` 配对的老理由作废 ✓ 残留一个中文串都进不了候选才是重点 ✓）。
const js = readFileSync(VIEWER_JS, "utf8");
// **词表在独立资产里**（NEW-H1 ✓）—— 抽表全部改读这份 ✓（锚点检查仍读主脚本 ✓）。
const dictJs = readFileSync(I18N_JS, "utf8");
const head = rawChunk(rs, "PAGE_HEAD");
const tailA = rawChunk(rs, "PAGE_TAIL_A");
const tailB = rawChunk(rs, "PAGE_TAIL_B");
// **记住"拼接后每个字符来自 viewer.rs 的哪个偏移"** ✓ ⇒ 报错时给**真的行号** ✓
//（若直接对 viewer.rs 做 `indexOf(文案)`，会先撞到**注释里**的同名文案 ✗ ——
//  实测「删除」被报成第 144 行（注释 `**删除确认**` ✓），而真身在第 156 行 ✓）。
const parts = [
  { body: head.body, start: head.start },
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
const textStart = dictJs.indexOf("const I18N_EN_TEXT");
const attrStart = dictJs.indexOf("const I18N_EN_ATTR");
if (textStart < 0 || attrStart < 0 || attrStart <= textStart) {
  console.error("❌ 双语覆盖率判据无法运行：在 " + I18N_JS + " 里找不到 I18N_EN_TEXT / I18N_EN_ATTR 两张表");
  process.exit(2);
}
const entryRe = /"((?:[^"\\]|\\.)*)"\s*:\s*"((?:[^"\\]|\\.)*)"/g;
// **键要按 JS 字符串字面量解转义** ✓：表里写 `"…快捷键 \\ 也可…"` ✓ ⇒ 运行时那个键**是一个反斜杠** ✓
// 而模板里的属性值也是**一个反斜杠** ✓ ⇒ 只把 `\"` 还原成 `"` 会把 `\\` 留在串里 ✗
// ⇒ 判据报"漏词条"的**假红** ✓（实测抓到过 ✗）。解不出来就退回原串 ✓（宁可多报一条 ✓，不静默 ✓）。
const decodeKey = (literal) => {
  try {
    return JSON.parse('"' + literal + '"');
  } catch {
    return literal.replace(/\\"/g, '"');
  }
};
const dict = new Map();
let entry;
while ((entry = entryRe.exec(dictJs.slice(textStart, attrStart)))) {
  dict.set(decodeKey(entry[1]), entry[2]);
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

// ★ **属性文案也要有词条** ✓（`title` / `placeholder` / `aria-label` ✓）。
//
// **为什么这一段是后加的** ✗：本判据原先**明说不查属性**（见文件头 ✓）⇒ 于是模板里
// **41 条中文 tooltip 一条英文词条都没有** ✓ 而判据**一直是绿的** ✗ ——
// 英文界面下按钮文字翻成了英文 ✓、**鼠标悬停整句中文** ✗。
// `browser-i18n.mjs` 也抓不到它 ✗：它同样只看**可见文本** ✓，tooltip 不进 DOM 文本 ✓。
// ⇒ "不检查"写进注释 ≠ 安全 ✓：**没判据守的那一半就是会烂掉** ✓（本项目的老病 ✓）。
const attrEnd = dictJs.indexOf("\n};", attrStart);
if (attrEnd < 0) {
  console.error("❌ 双语覆盖率判据无法运行：I18N_EN_ATTR 的表尾找不到");
  process.exit(2);
}
const attrDict = new Map();
entryRe.lastIndex = 0;
while ((entry = entryRe.exec(dictJs.slice(attrStart, attrEnd)))) {
  attrDict.set(decodeKey(entry[1]), entry[2]);
}
if (attrDict.size === 0) {
  console.error("❌ 双语覆盖率判据无法运行：I18N_EN_ATTR 一条词条都没解析出来（表被改写了？）");
  process.exit(2);
}
const decodeEntities = (value) =>
  value
    .replace(/&quot;/g, '"')
    .replace(/&#0?39;/g, "'")
    .replace(/&apos;/g, "'")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&#(\d+);/g, (_all, code) => String.fromCodePoint(Number(code)))
    .replace(/&amp;/g, "&");
const attrCandidates = new Map(); // 属性文案 ⇒ 行号 ✓
const attrRe = /(?:^|\s)(title|placeholder|aria-label)\s*=\s*"([^"]*[\u4e00-\u9fff][^"]*)"/g;
let attrHit;
while ((attrHit = attrRe.exec(visible))) {
  const decoded = decodeEntities(attrHit[2]).trim();
  if (!decoded || attrCandidates.has(decoded)) continue;
  const at = visibleFrom[attrHit.index + (attrHit[0].length - attrHit[2].length - 2)];
  attrCandidates.set(decoded, at === null || at === undefined ? null : rs.slice(0, at).split("\n").length);
}
// **自检**：抽不到足够多的属性文案 ⇒ 解析与模板脱节 ⇒ 报错，不给"0 条 ⇒ 绿" ✗。
if (attrCandidates.size < 20) {
  console.error("❌ 双语覆盖率判据无法运行：只从模板里抽出 " + attrCandidates.size + " 条中文属性文案（预期远多于此）⇒ 抽取逻辑已与模板脱节");
  process.exit(2);
}
for (const [text, line] of attrCandidates) {
  if (attrDict.has(text)) continue;
  problems.push(`属性漏词条：${line ? VIEWER_RS + ":" + line + " " : ""}${JSON.stringify(text)}`);
}
for (const [zh, en] of attrDict) {
  if (!en.trim()) problems.push(`属性词条非法：${JSON.stringify(zh)} 的英文是空的`);
  else if (en === zh) problems.push(`属性词条非法：${JSON.stringify(zh)} 的英文与中文一模一样（等于没翻 ✓）`);
  else if (CJK.test(en)) problems.push(`属性词条非法：${JSON.stringify(zh)} ⇒ ${JSON.stringify(en)} 里仍有汉字`);
}

// ★ **其余 7 种目标语言逐一对账** ✓（2026-11-17 扩 ✓）。
//
// **为什么**：上面各段只守英文 ✗ ⇒ 新语言的表完全可能缺条、留空、或整条照抄中文而判据全绿 ✗
//（与当年"属性文案没人守"是同一个病 ✓）。对账口径与英文一致：
//   ① **键集与 EN 精确相等**（缺一条 ⇒ 点名报缺哪条 ✓；多一条 ⇒ 点名报多哪条 ✓）；
//   ② **零空译** ✓；③ 值不得与中文键相同 ✗ —— 除白名单（见下）；
//   ④ 非日语的值里不得残留汉字 ✓（日语的值本就含汉字 ✗ 此条对它不成立 ⇒ 跳过 ✓；
//      韩语用谚文、不在汉字区段 ✓ 照常查 ✓）。
// **白名单**（值与中文键相同也算合法 ✓）只有两种东西：
//   · `2 1/4` / `3 1/8` 是**语言中立的尺寸记法** ✓ 七种语言的 TEXT 里都原样 ✓；
//   · 日语与中文**共享汉字词**（下限／上限／色相…）✓ 日语里就是这么写 ✗ 不是没翻 ✓。
// 白名单**双向守** ✓：实测相同却不在名单里 ⇒ 红 ✓；名单里有、实测已不相同 ⇒ 也红
//（防清单腐化 ✓ 与上面 DYNAMIC 同一条规矩 ✓）。
const LANG_TABLES = [
  { code: "fr", prefix: "FR" },
  { code: "es", prefix: "ES" },
  { code: "pt", prefix: "PT" },
  { code: "ja", prefix: "JA" },
  { code: "ko", prefix: "KO" },
  { code: "ru", prefix: "RU" },
  { code: "de", prefix: "DE" },
];
const IDENTICAL_OK_COMMON_TEXT = ["2 1/4", "3 1/8"];
const IDENTICAL_OK = {
  TEXT: {
    fr: IDENTICAL_OK_COMMON_TEXT,
    es: IDENTICAL_OK_COMMON_TEXT,
    pt: IDENTICAL_OK_COMMON_TEXT,
    ko: IDENTICAL_OK_COMMON_TEXT,
    ru: IDENTICAL_OK_COMMON_TEXT,
    de: IDENTICAL_OK_COMMON_TEXT,
    ja: [...IDENTICAL_OK_COMMON_TEXT, "下限", "上限", "色相", "半径", "方法", "形状", "矩形", "中心 x", "中心 y", "角度", "操作", "位置", "水彩（v2）", "名"],
  },
  ATTR: { ja: ["矩形"] },
};
function parseLangTable(name) {
  const start = dictJs.indexOf("const " + name);
  if (start < 0) return null;
  const end = dictJs.indexOf("\n};", start);
  if (end < 0) return null;
  const table = new Map();
  const re = new RegExp(entryRe.source, "g");
  let hit;
  while ((hit = re.exec(dictJs.slice(start, end)))) table.set(decodeKey(hit[1]), hit[2]);
  return table;
}
const langSummary = [];
for (const { code, prefix } of LANG_TABLES) {
  for (const [kind, enDict] of [["TEXT", dict], ["ATTR", attrDict]]) {
    const name = `I18N_${prefix}_${kind}`;
    const table = parseLangTable(name);
    if (!table) {
      problems.push(`缺表：${I18N_JS} 里找不到 ${name}（语言 ${code} 的${kind === "TEXT" ? "文本" : "属性"}词表不存在）`);
      continue;
    }
    if (table.size === 0) {
      console.error("❌ 多语言覆盖率判据无法运行：" + name + " 一条词条都没解析出来（表被改写了？）");
      process.exit(2);
    }
    for (const key of enDict.keys()) {
      if (!table.has(key)) problems.push(`${code} ${kind} 缺词条：${JSON.stringify(key)}（EN 有、${code} 没有 ⇒ 该语言下这条文案会落回中文）`);
    }
    for (const key of table.keys()) {
      if (!enDict.has(key)) problems.push(`${code} ${kind} 多词条：${JSON.stringify(key)}（EN 没有、${code} 多出 ⇒ 键集与 EN 不一致）`);
    }
    const okIdentical = new Set((IDENTICAL_OK[kind] && IDENTICAL_OK[kind][code]) || []);
    const seenIdentical = new Set();
    for (const [zh, value] of table) {
      if (!value.trim()) {
        problems.push(`${code} ${kind} 词条非法：${JSON.stringify(zh)} 的译文是空的`);
        continue;
      }
      if (value === zh) {
        seenIdentical.add(zh);
        if (!okIdentical.has(zh)) problems.push(`${code} ${kind} 词条非法：${JSON.stringify(zh)} 的译文与中文一模一样（等于没翻 ✓；若属语言中立记法或日语共享汉字词，把它加进 IDENTICAL_OK 并写明理由）`);
      } else if (code !== "ja" && CJK.test(value)) {
        problems.push(`${code} ${kind} 词条非法：${JSON.stringify(zh)} ⇒ ${JSON.stringify(value)} 里仍有汉字`);
      }
    }
    for (const key of okIdentical) {
      if (!seenIdentical.has(key)) problems.push(`白名单过期：IDENTICAL_OK 里 ${code} ${kind} 的 ${JSON.stringify(key)} 已不再是"译文与中文相同" ⇒ 删掉那一条（否则白名单会变成谎话 ✗）`);
    }
    if (kind === "TEXT") langSummary.push(`${code} TEXT ${table.size} 条`);
    else langSummary[langSummary.length - 1] += `｜ATTR ${table.size} 条`;
  }
}

console.log(`  模板里会进文本节点的中文文案：${candidates.size} 条｜I18N_EN_TEXT 词条：${dict.size} 条｜运行时会改写的初值：${DYNAMIC.length} 条`);
console.log(`  模板里的中文属性文案：${attrCandidates.size} 条｜I18N_EN_ATTR 词条：${attrDict.size} 条`);
console.log(`  其余 7 种语言逐一对账（键集与 EN 精确相等、零空译）：${langSummary.join("；")}`);
console.log("  ⚠️ 覆盖范围：模板的文本节点 ✓ 与 title/placeholder/aria-label 属性 ✓；**不看** viewer-app.js 运行时拼的文案 ✗（那只有真浏览器能验 ✓）。");
if (problems.length) {
  console.log(`  ✗ 界面文案的多语言覆盖率不合格（${problems.length} 条）：`);
  for (const problem of problems.slice(0, 20)) console.log("     - " + problem);
  if (problems.length > 20) console.log(`     … 另有 ${problems.length - 20} 条`);
  console.log("  结论：新加的界面文案必须在每个目标语言的表里都有词条 ✗ ⇒ 否则对应语言下会漏出中文 ✓。");
  process.exit(1);
}
console.log("  ✓ 模板里每一条会成为文本节点的中文文案在 8 个目标语言里都有词条，且词条不是占位；7 种新语言的键集与 EN 精确相等、零空译");
