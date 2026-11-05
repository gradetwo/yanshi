#!/usr/bin/env node
// **运行期可见文案的英文覆盖：静态棘轮**（第 1381 轮 ✓）
//
// **⇒ 这类缺陷是怎么暴露的** ✗：CI 上 `browser-layer-lock.mjs` 红了 ——
//   `refuseLockedLayer` 的 `log(…, "#c33")` 在英文模式下把整句中文写进了 `#stageNotice` 浮层 ✓。
//   两个人肉修了那一句（补 `m42` ✓），而**同类还有一百来处** ✗ —— 它们**没有判据**，
//   ⇒ 只能等下一次有人在浏览器里恰好切到英文、恰好触发那条错误 ✓（靠运气 ✗）。
//
// **⇒ 为什么 `#c33` / `forceNotice` 是「用户可见」** ✓（读实现，不是猜 ✓）：
//   `viewer-app.js` 的 `function log(line, cls, forceNotice)` 里一行是
//     `if (cls === "#c33" || forceNotice === true) stageNotice(line);`
//   ⇒ 凡走 `#c33`（错误色）或显式 `forceNotice === true` 的行，**同时**写进 `#stageNotice` 浮层 ✓。
//   而 `#stageNotice` 是**唯一**在 `#log` 折叠时仍一定看得见的报错去处 ✓（该函数自己的注释写着 ✓）。
//   ⇒ 所以这三层里出现中文，`lang=en` 下就是**用户面前的中文** ✗。
//   `stageNotice(<中文>)` 直调同理（它本身就是那块浮层 ✓）。
//
// **⇒ 静态解析，不开浏览器** ✓：判据只读两个资产文件 + 词表，结论与机器无关 ✓。
//
// **⇒ 判什么（三层，逐层打印计数）**：
//   ① `log(<中文…>, …, "#c33")`  —— 中文且**没有任何词条覆盖** ⇒ 缺口 ✓
//   ② `log(<中文…>, …, …, true)` —— `forceNotice === true`，同上口径 ✓
//   ③ `stageNotice(<中文…>)` 直调，同上口径 ✓
//   「被覆盖」= **整句**在 `I18N_EN_TEXT` 里 ✓，**或**能被 `viewer-app.js` 里某条 `mNN` 句式命中 ✓。
//   ⚠ 只查 `I18N_EN_TEXT` ＋ `mNN`，**不查** `I18N_EN_ATTR` / `I18N_EN_ATTR_PATTERNS` ✗：
//     那两张表由**属性路径**（`translateAttr`）消费 ✓，而这三层写的是**文本节点** ✓
//     ⇒ 拿属性词条来「覆盖」文本是一句谎话（同 AGENTS.md「核对我们断的是同一个取值集合」✓）。
//
// **⇒ 拼了数据的句子怎么判**（这是本判据的关键 ✓，不许因为插值就跳过 ✗）：
//   把 `+` / 模板串拆成**片段**：字面量原样拼 ✓，动态片段换成**占位 `1`** ✓
//   （`1` 同时能被 `(\d+)` / `(\S+)` / `(.+)` 吃下 ✓）；
//   动态片段**内部**出现的字符串字面量另算一种候选 ✓（`what || "这一步"` ⇒ 候选取 `"这一步"` ✓，
//   这样 `m42` 的 `(画布上的一笔|移动对象|这一步)` 才配得上 ✓）。候选按笛卡尔积展开，上限 16 组 ✓。
//   ⇒ 全部候选都配不上 ⇒ **缺口** ✓（不再有「因为插值所以判不了」的例外 ✓）。
//   ⇒ 唯一进「动态」桶的是 **`log(message, "#c33")` 这种 arg0 里一个字符串字面量都没有** 的调用 ✓：
//     它的文案在别处拼好（函数参数）⇒ 静态看不到 ⇒ 单独计数并**打印**出来 ✓，**不计入缺口** ✓（不静默丢 ✓）。
//
// **⇒ 棘轮** ✓：`UNCOVERED_CEILING` 是**判据自己造的期望值**（只许降 ✓），不是被测系统的数量
//   （AGENTS.md「判据里的数字分两类」✓）。改它必须写理由 ✓（同 `check-index-html-byte-budget.mjs` ✓）。
//
// **⇒ 允许名单** `scripts/i18n-runtime-allowlist.txt`：只登记「确实是开发者可见、不该翻」的 ✓，
//   每条必须带原因 ✓，且**过期就红** ✓（名单里指的缺口已经没了／覆盖了 ⇒ 删掉那一行 ✓；
//   同 `check-pwa-local-params.mjs` 的「无过期条目」✓）。
//
// **⇒ 变异**（验它能红 ✓）：把 `UNCOVERED_CEILING` 减 1 ⇒ 退出码 1 ✓；还原 ⇒ 0 ✓。
//
// 用法：node scripts/check-i18n-runtime-coverage.mjs
// 退出码：0 = 缺口 ≤ 棘轮且名单不过期｜1 = 缺口变多／名单过期或没写原因｜3 = 前提不成立（不可判）
import { readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const VIEWER = "crates/yanshi-http/assets/viewer-app.js";
const I18N = "crates/yanshi-http/assets/i18n.js";
const ALLOWLIST = "scripts/i18n-runtime-allowlist.txt";

// 棘轮上界（基线 2026-11-04：三层合计缺口 **135** 条 = `#c33` 131 ＋ `forceNotice` 4 ＋ `stageNotice` 0）。
//   上一次是 136（`#c33` 132）⇒ 补 `m43`（`没有 WASM 内核，无法自检`）后降 1 ✓。
// ⇒ 这个数**只许降** ✓：每修一批就把 `UNCOVERED_CEILING` 改到新的实测值 ✓。
// ⇒ **要涨**必须①抬值 ②在提交信息里写清「谁涨了多少、为什么、打算什么时候修」✓
//   —— 让膨胀**必须被看见** ✓（这就是本判据的全部目的 ✓，同 `check-index-html-byte-budget.mjs` ✓）。
// 数它用 `I18N_COVERAGE_VERBOSE=1 node scripts/check-i18n-runtime-coverage.mjs`（清单条数就是它 ✓）。
// 基线怎么来的：`browser-layer-lock.mjs` 红在锁定层那句（已补 `m42` ✓）⇒ 顺着 `log()` 的实现
//   发现「`#c33` / `forceNotice` 都进 `#stageNotice` 浮层」⇒ 把那一类**全部**扫了一遍 ✓。
//   ⇒ 与「105 行」的关系：那个数是**行**口径（`grep` 一行里同时有 `log(` 与 `#c33`，实测 111 行）；
//     本条是**调用**口径（多行调用按 1 处算 ✓）⇒ 132 比 111 多出的部分是**跨行**的那几句 ✓。
// 2026-11-05：PWA 打开"本地还没有的 doc"时要补写 `create_document`（否则内核认为文档 0×0、
//   画布全白）⇒ 补写结果要留痕 ⇒ 新增一条 `#c33` 诊断日志 ⇒ 缺口 135→**136**。
//   **∴ 为什么补词条而不抬棘轮 ✗**：本资产开头写明"日志与诊断**不翻译**（它们是给人看的长句）"✓
//   ⇒ 这条注定进不了词表 ✓；而它带**动态尺寸/失败原因**（`1024×1024`、内核给的 detail ✓）
//   ⇒ 也不适合造句式 ✓。**∴ 抬 1 条并留下理由**（判据自己的提示允许这条路 ✓）。
//   **∴ 想把它降回去**：把那两条日志并进已有的「错误 …」通用形状，或等这批诊断统一改写。
const UNCOVERED_CEILING = 136;

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};
const undecidable = (why) => {
  console.error(`  ⊘ 不可判：${why}（不是产品问题）`);
  process.exit(3);
};

// ---------------------------------------------------------------- 源码扫描工具
// 只做「够用」的扫描 ✓：字符串 / 模板串（含 `${}`）/ 注释 / 正则字面量 ✓。
// ⇒ 不引第三方解析器（AGENTS.md 依赖原则 ✓ 这里收益不抵代价 ✓）。
const isWordChar = (c) => /[A-Za-z0-9_$]/.test(c);

function skipQuoted(src, i) {
  const q = src[i];
  i++;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\") { i += 2; continue; }
    if (c === q) return i + 1;
    i++;
  }
  return i;
}

function skipTemplate(src, i) {
  i++;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\") { i += 2; continue; }
    if (c === "`") return i + 1;
    if (c === "$" && src[i + 1] === "{") { i = skipBraced(src, i + 1); continue; }
    i++;
  }
  return i;
}

function skipBraced(src, i) {
  let depth = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === '"' || c === "'" || c === "`") { i = c === "`" ? skipTemplate(src, i) : skipQuoted(src, i); continue; }
    if (c === "/" && src[i + 1] === "/") { const nl = src.indexOf("\n", i); i = nl < 0 ? src.length : nl; continue; }
    if (c === "/" && src[i + 1] === "*") { const e = src.indexOf("*/", i + 2); i = e < 0 ? src.length : e + 2; continue; }
    if (c === "{") depth++;
    else if (c === "}") { depth--; if (depth === 0) return i + 1; }
    i++;
  }
  return i;
}

function skipRegex(src, i) {
  i++;
  let inClass = false;
  while (i < src.length) {
    const c = src[i];
    if (c === "\\") { i += 2; continue; }
    if (c === "[") inClass = true;
    else if (c === "]") inClass = false;
    else if (c === "/" && !inClass) { i++; while (i < src.length && /[a-z]/.test(src[i])) i++; return i; }
    else if (c === "\n") return i;
    i++;
  }
  return i;
}

/// 上一个「有效字符」（跳过空白），用来判断 `/` 是正则还是除号 ✓。
function prevSignificant(src, i) {
  let j = i - 1;
  while (j >= 0 && /\s/.test(src[j])) j--;
  return j < 0 ? "" : src[j];
}

/// 把 `log(` / `stageNotice(` 的调用点全部找出来（跳过注释、字符串、正则）✓。
function findCalls(src, name) {
  const out = [];
  let i = 0;
  let prevChar = "";
  let prevWord = "";
  while (i < src.length) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") { const nl = src.indexOf("\n", i); i = nl < 0 ? src.length : nl; continue; }
    if (c === "/" && src[i + 1] === "*") { const e = src.indexOf("*/", i + 2); i = e < 0 ? src.length : e + 2; continue; }
    if (c === '"' || c === "'") { i = skipQuoted(src, i); prevChar = "x"; prevWord = ""; continue; }
    if (c === "`") { i = skipTemplate(src, i); prevChar = "x"; prevWord = ""; continue; }
    if (c === "/" && /[=(,:[!&|?{};\n]/.test(prevChar)) { i = skipRegex(src, i); prevChar = "x"; prevWord = ""; continue; }
    if (isWordChar(c)) {
      let j = i;
      while (j < src.length && isWordChar(src[j])) j++;
      const word = src.slice(i, j);
      let k = j;
      while (k < src.length && /\s/.test(src[k])) k++;
      const before = prevSignificant(src, i);
      // 调用点：名字后面紧跟 `(`，且前面不是 `.`（排除 `x.log(`）也不是 `function`（排除定义 ✓）
      // ⚠ 判 `function` 要拿**上一个词** ✓ —— 拿「从上一个词末尾到这里的空白」只会得到空白 ✗
      //   （那个 bug 让 `function stageNotice(text)` 的定义被当成调用点 ✓，第 1381 轮实测 ✓）
      if (word === name && src[k] === "(" && before !== "." && prevWord !== "function") {
        out.push({ start: i, argStart: k + 1, argEnd: matchParen(src, k), text: src.slice(i, k) });
      }
      i = j; prevChar = "a"; prevWord = word; continue;
    }
    if (!/\s/.test(c)) { prevChar = c; prevWord = ""; }
    i++;
  }
  return out;
}

/// `(` 在 i 处，返回配对 `)` 的下标 ✓。
function matchParen(src, i) {
  let depth = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === '"' || c === "'") { i = skipQuoted(src, i); continue; }
    if (c === "`") { i = skipTemplate(src, i); continue; }
    if (c === "/" && src[i + 1] === "/") { const nl = src.indexOf("\n", i); i = nl < 0 ? src.length : nl; continue; }
    if (c === "/" && src[i + 1] === "*") { const e = src.indexOf("*/", i + 2); i = e < 0 ? src.length : e + 2; continue; }
    if (c === "(") depth++;
    else if (c === ")") { depth--; if (depth === 0) return i; }
    i++;
  }
  return src.length;
}

/// 顶层按 `sep` 切（跳过字符串/模板/注释/括号嵌套）✓。
function splitTopLevel(expr, sep) {
  const parts = [];
  let depth = 0;
  let from = 0;
  let i = 0;
  while (i < expr.length) {
    const c = expr[i];
    if (c === '"' || c === "'") { i = skipQuoted(expr, i); continue; }
    if (c === "`") { i = skipTemplate(expr, i); continue; }
    if (c === "(" || c === "[" || c === "{") depth++;
    else if (c === ")" || c === "]" || c === "}") depth--;
    else if (c === sep && depth === 0) { parts.push(expr.slice(from, i)); from = i + 1; }
    i++;
  }
  parts.push(expr.slice(from));
  return parts.map((p) => p.trim()).filter((p) => p.length > 0);
}

/// 一个「纯字面量」片段 ⇒ 它的文本；不是纯字面量 ⇒ null ✓。
function literalOf(part) {
  const t = part.trim();
  if (t.length >= 2 && (t[0] === '"' || t[0] === "'")) {
    const end = skipQuoted(t, 0);
    if (end !== t.length) return null;
    return unquote(t.slice(1, -1));
  }
  if (t.length >= 2 && t[0] === "`") {
    const end = skipTemplate(t, 0);
    if (end !== t.length) return null;
    const body = t.slice(1, -1);
    if (body.includes("${")) return null;
    return unquote(body);
  }
  return null;
}

function unquote(s) {
  return s.replace(/\\(u\{?[0-9a-fA-F]+\}?|x[0-9a-fA-F]{2}|.)/g, (all, esc) => {
    if (esc[0] === "u") {
      const hex = esc.replace(/[u{}]/g, "");
      return String.fromCodePoint(parseInt(hex, 16));
    }
    if (esc[0] === "x") return String.fromCharCode(parseInt(esc.slice(1), 16));
    return ({ n: "\n", t: "\t", r: "\r", "\\": "\\", '"': '"', "'": "'", "`": "`" })[esc] ?? esc;
  });
}

/// 片段内部出现的字符串字面量（按出现顺序）✓ —— 给动态片段当候选值 ✓。
function literalsInside(part) {
  const out = [];
  let i = 0;
  while (i < part.length) {
    const c = part[i];
    if (c === '"' || c === "'") {
      const end = skipQuoted(part, i);
      const body = part.slice(i + 1, end - 1);
      if (!body.includes("${")) out.push(unquote(body));
      i = end;
      continue;
    }
    if (c === "`") {
      const end = skipTemplate(part, i);
      const body = part.slice(i + 1, end - 1);
      if (!body.includes("${")) out.push(unquote(body));
      i = end;
      continue;
    }
    i++;
  }
  return out;
}

const CJK = /[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]/;
const PLACEHOLDER = "1";
const DYNAMIC_MARK = "…";
const MAX_CANDIDATES = 16;

/// 模板串的**内脏**切成「字面量片段」与「`${}` 表达式」两类 ✓。
/// ⇒ 少了这一步，`\`自检失败：本地 ${a} 字节…\`` 会被当成「一个动态片段」✗
///    ⇒ 它的中文框架就**静默消失**（本判据自己的第 1 种静默降级 ✓）。
function templatePieces(body) {
  const out = [];
  let buf = "";
  let i = 0;
  while (i < body.length) {
    const c = body[i];
    if (c === "\\") { buf += body.slice(i, i + 2); i += 2; continue; }
    if (c === "$" && body[i + 1] === "{") {
      if (buf) { out.push({ lit: unquote(buf) }); buf = ""; }
      const end = skipBraced(body, i + 1);
      out.push({ expr: body.slice(i + 2, end - 1) });
      i = end;
      continue;
    }
    buf += c;
    i++;
  }
  if (buf) out.push({ lit: unquote(buf) });
  return out;
}

/// 把一个表达式还原成「待匹配的框架」✓（见文件头「拼了数据的句子怎么判」✓）。
/// 返回 `{ candidates, display }`：
///   `candidates` 是拿去 `test()` 的**候选串**（动态槽取 `1` 或片段内的字面量 ✓）
///   `display` 是给人看的框架（动态槽写 `…` ✓）—— 指纹与分类都基于 candidates[0] ✓。
function frameworksOf(expr) {
  const slots = [];
  const pushLiteral = (text) => slots.push({ options: [text], display: text });
  const pushDynamic = (source) => {
    const inner = literalsInside(source).filter((x) => x && x !== PLACEHOLDER);
    slots.push({ options: [PLACEHOLDER, ...inner], display: DYNAMIC_MARK });
  };
  for (const part of splitTopLevel(expr, "+")) {
    const t = part.trim();
    if (t.startsWith("`") && skipTemplate(t, 0) === t.length) {
      for (const piece of templatePieces(t.slice(1, -1))) {
        if (piece.lit !== undefined) pushLiteral(piece.lit); else pushDynamic(piece.expr);
      }
      continue;
    }
    const lit = literalOf(t);
    if (lit !== null) pushLiteral(lit);
    else pushDynamic(t);
  }
  let candidates = [""];
  for (const slot of slots) {
    const next = [];
    for (const prefix of candidates) for (const option of slot.options) next.push(prefix + option);
    candidates = next.length > MAX_CANDIDATES ? next.slice(0, MAX_CANDIDATES) : next;
  }
  return { candidates, display: slots.map((slot) => slot.display).join("") };
}

// ---------------------------------------------------------------- 前提：读源文件
let viewer;
try { viewer = readFileSync(VIEWER, "utf8"); } catch {
  undecidable(`读不到 ${VIEWER}`);
}
let i18nSource;
try { i18nSource = readFileSync(I18N, "utf8"); } catch {
  undecidable(`读不到 i18n 词条文件 ${I18N}`);
}

// `log()` 的定义必须真在，并且语义必须还是「#c33 / forceNotice ⇒ 浮层」✓
//   ⇒ 这三条是本判据的**前提** ✓：少了任何一条，「这三层用户可见」就不再成立 ⇒ 应当判不了 ✓。
const logDef = /function\s+log\s*\(\s*[A-Za-z_$][\w$]*\s*,\s*([A-Za-z_$][\w$]*)\s*,\s*([A-Za-z_$][\w$]*)\s*\)/.exec(viewer);
if (!logDef) undecidable(`在 ${VIEWER} 里找不到 \`function log(line, cls, forceNotice)\` 定义`);
const [, clsName, noticeName] = logDef;
// ⚠ `{` 要按**花括号**配对（`matchParen` 只认圆括号 ⇒ 那会让 body 落到函数外面 ✗）
const logBodyStart = viewer.indexOf("{", logDef.index);
const logBodyEnd = skipBraced(viewer, logBodyStart);
const logBody = viewer.slice(logBodyStart, logBodyEnd);
if (!new RegExp(`${clsName}\\s*===\\s*"#c33"`).test(logBody) ||
    !new RegExp(`${noticeName}\\s*===\\s*true`).test(logBody)) {
  undecidable(`\`log()\` 的定义里不再有 \`${clsName} === "#c33" || ${noticeName} === true ⇒ stageNotice\`（语义变了 ⇒ 本判据前提不成立）`);
}
const stageNoticeDef = /function\s+stageNotice\s*\(/.exec(viewer);
if (!stageNoticeDef) undecidable(`在 ${VIEWER} 里找不到 \`function stageNotice(\`) 定义`);

// 词表：真跑（不是正则抄一份 ✓）
let tables;
try {
  tables = new Function(`${i18nSource}\nreturn { TEXT: I18N_EN_TEXT, ATTR: I18N_EN_ATTR };`)();
} catch (error) {
  undecidable(`i18n 词条文件求值失败（${String(error && error.message ? error.message : error).slice(0, 120)}）`);
}
if (!tables || typeof tables.TEXT !== "object" || tables.TEXT === null) {
  undecidable("i18n 词条文件里取不到 `I18N_EN_TEXT` 对象");
}
const exactDict = tables.TEXT;
const attrDict = tables.ATTR && typeof tables.ATTR === "object" ? tables.ATTR : {};

// `mNN` 句式表：从 `const mNN = /…/.exec(zh);` 抽出来当场 `new RegExp` ✓
const patterns = [];
{
  const re = /\bconst\s+(m\d+)\s*=\s*\//g;
  let hit;
  while ((hit = re.exec(viewer)) !== null) {
    const srcEnd = skipRegex(viewer, hit.index + hit[0].length - 1);
    const literal = viewer.slice(hit.index + hit[0].length - 1, srcEnd);
    const tail = viewer.slice(srcEnd, srcEnd + 24);
    if (!/^\s*\.exec\s*\(\s*\w+\s*\)/.test(tail)) continue; // 只认 `.exec(zh)` 那一族 ✓
    try {
      patterns.push({ name: hit[1], re: new RegExp(literal.slice(1, literal.lastIndexOf("/"))) });
    } catch (error) {
      undecidable(`句式 ${hit[1]} 无法 new RegExp（${literal.slice(0, 40)}）`);
    }
  }
}
if (patterns.length === 0) undecidable(`在 ${VIEWER} 里找不到 \`const mNN = /…/.exec(zh)\` 句式表`);

// ---------------------------------------------------------------- 三层调用点
const logCalls = findCalls(viewer, "log");
const noticeCalls = findCalls(viewer, "stageNotice");

const records = [];
const noLiteralRecords = [];
const lineOf = (index) => viewer.slice(0, index).split("\n").length;

/// 收集一处调用 ✓：`display` 里有汉字 ⇒ 可判 ✓；否则若还带动态槽 ⇒ 文案在变量里
/// （静态看不到 ⇒ 进「动态」桶，**打印**出来 ✓，不静默丢 ✓）。
function collect(call, kind, expression) {
  const { candidates, display } = frameworksOf(expression);
  const dynamic = display.includes(DYNAMIC_MARK);
  if (CJK.test(display)) {
    records.push({ kind, line: lineOf(call.start), candidates, text: candidates[0], display });
  } else if (dynamic) {
    noLiteralRecords.push({ kind, line: lineOf(call.start), expression: expression.trim().slice(0, 80) });
  }
}
for (const call of logCalls) {
  const args = splitTopLevel(viewer.slice(call.argStart, call.argEnd), ",");
  if (args.length === 0) continue;
  const isC33 = args.slice(1).some((a) => /^(["'])#c33\1$/.test(a.trim()));
  const isForce = args.length >= 3 && args[2].trim() === "true";
  if (!isC33 && !isForce) continue;
  collect(call, isC33 ? "c33" : "force", args[0]);
}
for (const call of noticeCalls) {
  // `log()` 肚子里那句 `stageNotice(line)` 是**通道本身** ✓ —— 它只是把 log 的那一行转过去，
  //   文案归上面那一层管 ✓ ⇒ 这里跳过，不重复记一次（否则每个 `#c33` 行都会多出一条噪声 ✓）。
  if (call.start > logBodyStart && call.start < logBodyEnd) continue;
  const args = splitTopLevel(viewer.slice(call.argStart, call.argEnd), ",");
  if (args.length === 0) continue;
  collect(call, "notice", args[0]);
}

// ---------------------------------------------------------------- 判定覆盖
const coveredBy = (frameworks) => {
  for (const f of frameworks) {
    if (Object.prototype.hasOwnProperty.call(exactDict, f)) return `整句词条`;
    for (const pattern of patterns) if (pattern.re.test(f)) return `句式 ${pattern.name}`;
  }
  return null;
};

const perKind = { c33: { all: 0, covered: 0 }, force: { all: 0, covered: 0 }, notice: { all: 0, covered: 0 } };
const uncovered = [];
for (const record of records) {
  perKind[record.kind].all += 1;
  const via = coveredBy(record.candidates);
  if (via) { perKind[record.kind].covered += 1; continue; }
  uncovered.push(record);
}

// 缺口指纹：`kind:hash8` —— **不用行号** ✗（行号会随无关改动漂移，内核 ② ✓）
//   基线用 `candidates[0]`（= 全部动态槽取 `1` 的那一组，**最稳定**的一组 ✓）。
const fingerprint = (record) =>
  `${record.kind}:${createHash("sha1").update(`${record.kind}|${record.candidates[0]}`).digest("hex").slice(0, 8)}`;
for (const gap of uncovered) gap.key = fingerprint(gap);

// ---------------------------------------------------------------- 允许名单
let allowText = "";
try { allowText = readFileSync(ALLOWLIST, "utf8"); } catch {
  undecidable(`读不到允许名单 ${ALLOWLIST}`);
}
const allowed = new Map();
const allowProblems = [];
for (const raw of allowText.split("\n")) {
  const line = raw.trim();
  if (!line || line.startsWith("#")) continue;
  const [key, ...rest] = line.split(/\s+/);
  const reason = rest.join(" ").trim();
  if (!/^(c33|force|notice):[0-9a-f]{8}$/.test(key)) {
    allowProblems.push(`条目格式不对（应为 \`c33:xxxxxxxx <原因>\`）：${line.slice(0, 60)}`);
    continue;
  }
  if (!reason) { allowProblems.push(`条目没写原因：${key}`); continue; }
  if (allowed.has(key)) { allowProblems.push(`条目重复：${key}`); continue; }
  allowed.set(key, reason);
}

const gapKeys = new Set(uncovered.map((g) => g.key));
const staleAllow = [...allowed.keys()].filter((key) => !gapKeys.has(key));
const counted = uncovered.filter((g) => !allowed.has(g.key));

// ---------------------------------------------------------------- 按原因分类
// ⇒ 分类**只用来读**（给后续分批修的人看 ✓），不参与判定 ✓ —— 所以它宁可粗，不要假精确 ✓。
// 分类的**顺序就是优先级** ✓（一句里可能同时出现「服务端」和「导出」，归到更靠前的那类 ✓）。
const REASON_RULES = [
  [/内核|WASM|WebGPU|合成|指纹|自检/, "本地内核与渲染诊断"],
  [/导入|上传|介质|PSD 导入|示例画面/, "导入与上传"],
  [/导出|下载|回执|PNG|zip|已导出/, "导出与回执"],
  [/先勾选|请先|勾选|选区|拖出|再点|先选|先写|请只/, "勾选/选区等操作前置"],
  [/参数|必须|只能|不支持|缺少|填齐|合法|需要|格式|大于 0|越界|不能|空/, "参数与输入校验"],
  [/拒绝|服务端|错误码|permission_denied|连不上|离线|未发送/, "服务端与网络"],
];
const categoryOf = (text) => (REASON_RULES.find(([re]) => re.test(text)) || [null, "其他（面板回执/杂项）"])[1];
const categories = new Map();
for (const gap of counted) {
  const name = categoryOf(gap.display);
  categories.set(name, (categories.get(name) || 0) + 1);
}

// ---------------------------------------------------------------- 报告
check(true, `源码：${VIEWER}（${viewer.split("\n").length} 行）；句式 ${patterns.length} 条；整句词条 ${Object.keys(exactDict).length} 条（属性词条 ${Object.keys(attrDict).length} 条**不参与**文本判定）`);
check(perKind.c33.all + perKind.force.all + perKind.notice.all > 0,
  `三层调用点合计 ${perKind.c33.all + perKind.force.all + perKind.notice.all} 处（中文）`);
check(true, `① \`#c33\` 行：${perKind.c33.all} 处，已覆盖 ${perKind.c33.covered}，缺口 ${perKind.c33.all - perKind.c33.covered}`);
check(true, `② \`forceNotice === true\` 行：${perKind.force.all} 处，已覆盖 ${perKind.force.covered}，缺口 ${perKind.force.all - perKind.force.covered}`);
check(true, `③ \`stageNotice(<中文>)\` 直调：${perKind.notice.all} 处，已覆盖 ${perKind.notice.covered}，缺口 ${perKind.notice.all - perKind.notice.covered}`);
check(true, `动态（arg0 一个字面量都没有，文案在变量里 ⇒ 静态看不到）：${noLiteralRecords.length} 处 ⇒ 不计缺口，逐条列出`);
for (const record of noLiteralRecords) {
  console.log(`      · 第 ${record.line} 行 ${record.kind}：${record.expression}`);
}
check(true, `允许名单：${allowed.size} 条（每条带原因）`);

check(counted.length <= UNCOVERED_CEILING,
  `未覆盖缺口 ${counted.length} ≤ 棘轮 ${UNCOVERED_CEILING}`,
  counted.length > UNCOVERED_CEILING
    ? `超出 ${counted.length - UNCOVERED_CEILING} 条 ⇒ 要么补词条，要么带着理由抬棘轮`
    : `还剩 ${UNCOVERED_CEILING - counted.length} 条余量`);
check(staleAllow.length === 0,
  `允许名单无过期条目`,
  staleAllow.length ? `这些条目已不再对应缺口（修好了）⇒ 删掉：${staleAllow.join("、")}` : "");
check(allowProblems.length === 0, `允许名单条目格式与原因齐备`,
  allowProblems.length ? allowProblems.join("；") : "");

console.log(`  ── 缺口按原因分类（${counted.length} 条）──`);
for (const [name, count] of [...categories.entries()].sort((a, b) => b[1] - a[1])) {
  console.log(`      ${String(count).padStart(3)} 条  ${name}`);
}
if (process.env.I18N_COVERAGE_VERBOSE === "1") {
  console.log(`  ── 缺口清单（I18N_COVERAGE_VERBOSE=1；框架里的 … 是拼进去的数据 ✓）──`);
  for (const gap of counted) console.log(`      ${gap.key} 第 ${gap.line} 行：${gap.display.slice(0, 90)}`);
}

if (failures.length) {
  console.error(`✗ check-i18n-runtime-coverage：${failures.length} 项不通过`);
  process.exit(1);
}
console.log(`✓ check-i18n-runtime-coverage：缺口 ${counted.length} 条在棘轮 ${UNCOVERED_CEILING} 之内，允许名单 ${allowed.size} 条无过期`);
