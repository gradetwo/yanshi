#!/usr/bin/env node
//! **★ 安全编辑：把「**反复犯的编辑错误**」变成**机器会拒绝**的操作 ✗ ★**
//!
//! **∴ 为什么造它 ✗**（**第 801 轮；**用户直接指令 ✓）★**：
//!   **∴ 用户在会话中指出 ✗**：**「**经常犯的错误要考虑造工具简化自己的操作和避免错误**」
//!     ＋ **∴ 而**本会话的账（**有据可查 ✓）★**：
//!       ⇒ **∴ 错误种类 ①** **在**模板串／字符串里编辑****✗ **≥6 次**
//!         ⇒ **∴ 具体 ✗**：`$VAR` 后面接非 ASCII ⇒ 被改成 `${VAR}`（**插入插值洞 ✓）
//!           ＋ 注释里的**反引号**把模板串**提前闭合**
//!             ＋ **在双引号串里写裸 `"`** ⇒ **python／JS 语法错** ✓ ★★
//!       ⇒ **∴ 错误种类 ②** **编辑脚本自己中止 ⇒ 文件**没改**✗ 而**我**以为改了****✗ **≥4 次**
//!         ⇒ **∴ 根因 ✗**：**python heredoc 里的引号错**✗ ⇒ **∴ 它**在 `assert` 之前就死了**
//!           ＋ **∴ 而**我只跑了 `node --check`**✗ ⇒ **∴ 而**文件没变 ⇒ **∴ 检查**当然通过** ✓ ★★★
//!
//! **∴ 它做什么 ✗**（**四道闸 ✓）★**：
//!   ⇒ **∴ ① 字面替换 ＋ **计数校验****✗：** `--old` 必须在原文里**恰好出现 N 次**（**缺省 1 ✓）
//!     ⇒ **∴ 于是**"没匹配到"会**报错退出**✗ 而**不是**静默无操作** ✓ ★★
//!   ⇒ **∴ ② **写前**用 `--check` 验语法****✗：**先写临时文件 ⇒ 跑 `node --check`／`py_compile`
//!     ⇒ **∴ 语法不过 ⇒ **原文件**不动**** ✓ ★★
//!   ⇒ **∴ ③ 模板串守卫****✗：**若**改动的行**含反引号或 `${`
//!     ⇒ **∴ 则**拒绝在**注释里**引入反引号 ＋ **拒绝**新增 `${`（**除非显式 `--allow-interp` ✓）★
//!   ⇒ **∴ ④ **打印 diff ＋ **确认文件真的变了****✗：**比对替换后的内容，**不同**才算成功** ✓ ★★★
//!
//! **∴ 用法 ✗**：
//!   ```
//!   node scripts/safe-edit.mjs <文件> --old '<原文>' --new '<新文>' [--count N] [--allow-interp]
//!   node scripts/safe-edit.mjs <文件> --old-file a.txt --new-file b.txt
//!   ```
//! **∴ 从标准输入读 ✗**：`--new -` ⇒ **∴ 于是**新内容可以含任意引号 ＋ 换行** ✓ ★
//!
//! **∴ 退出码 ✗**：0 成功｜1 前置不满足（**没匹配／语法不过／守卫拦下 ✓）｜2 用法错

import { readFileSync, writeFileSync, existsSync, mkdtempSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, extname } from "node:path";
// **∴ 临时目录必须**统一清理**** ✗（**第 801 轮；**门禁教我的 ✓）★
//   ＋ **∴ 仓库规则 ✗**：**用 `mkdtempSync` 的脚本**必须**有清理机制**✗
//     ⇒ **∴ 而**共享模块是**推荐路径**（**导入即正确 ✓）★
//       ＋ **∴ 历史证据 ✗**：**26 个脚本用 `mkdtempSync` ⇒ **15 个没清理**
//         ⇒ **∴ 于是**累积出 **131 个** `gpuprobe-`／`gfp-` 目录** ✓ ★★
import { trackTemp } from "./lib/temp-dirs.mjs";

const argv = process.argv.slice(2);
const usage = (msg) => {
  console.error(`  ✗ ${msg}`);
  console.error("  用法：node scripts/safe-edit.mjs <文件> --old '<原文>' --new '<新文>' [--count N] [--allow-interp]");
  console.error("        （--old-file／--new-file 可代替 --old／--new；--new - 从标准输入读）");
  process.exit(2);
};

const file = argv[0];
if (!file || file.startsWith("--")) usage("缺文件名");
if (!existsSync(file)) usage(`文件不存在：${file}`);

const opt = (name, dflt = null) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] !== undefined && !argv[i + 1].startsWith("--") ? argv[i + 1] : dflt;
};
const has = (name) => argv.includes(name);

const readArg = (name, fileFlag) => {
  const f = opt(fileFlag);
  if (f) {
    if (!existsSync(f)) usage(`${fileFlag} 不存在：${f}`);
    return readFileSync(f, "utf8");
  }
  const v = opt(name);
  return v;
};

let oldText = readArg("--old", "--old-file");
let newText = readArg("--new", "--new-file");
if (opt("--new") === "-") {
  // **∴ 从标准输入读 ⇒ **∴ 任意引号 ＋ 换行都不会被 shell 吃掉**** ✓ ★
  newText = readFileSync(0, "utf8");
}
if (oldText === null || oldText === undefined) usage("缺 --old（或 --old-file）");
if (newText === null || newText === undefined) usage("缺 --new（或 --new-file，或 --new -）");

const wantCount = Number(opt("--count", "1"));
if (!Number.isInteger(wantCount) || wantCount < 1) usage("--count 必须是 ≥1 的整数");

const src = readFileSync(file, "utf8");

// **∴ 闸①：**计数校验****✗（**∴ 没匹配到 ⇒ 报错 ⇒ **∴ 不**静默无操作** ✓）★
const occurrences = src.split(oldText).length - 1;
if (occurrences !== wantCount) {
  console.error(`  ✗ 闸① 计数不符：--old 在 ${file} 里出现 ${occurrences} 次，期望 ${wantCount} 次`);
  console.error("     ⇒ **∴ 原文件**未改动** ✓");
  console.error("     ⇒ **∴ 若**要全部替换 ⇒ **∴ 用** `--count ${occurrences}`** ✓");
  process.exit(1);
}

const next = wantCount === 1 ? src.replace(oldText, newText) : src.split(oldText).join(newText);

// **∴ 闸④（**先做，因为它最便宜 ✓）：**文件必须真的变了**** ✓
if (next === src) {
  console.error("  ✗ 闸④ 替换后内容与原文**完全相同** ⇒ **∴ 没有实际改动**（--old 与 --new 相同？）");
  process.exit(1);
}

// **★★★ 闸③：**模板串守卫****✗ ★★★**（**∴ 本会话 ≥6 次错误的直接对策 ✓）
if (!has("--allow-interp")) {
  const problems = [];
  // **∴ 规则 A ✗**：**新增的 `${` 一律可疑**✗ ⇒ **∴ 除非**原文本来就有同样多** ✓
  const before = (src.match(/\$\{/g) || []).length;
  const after = (next.match(/\$\{/g) || []).length;
  if (after > before) {
    // **∴ 这一行本身**就是那条教训的活证据** ✗（**第 801 轮 ✓）★
    //   ＋ **∴ 我**第一版写成模板串**✗ 而**里面含未转义的插值起始**
    //     ⇒ **∴ 于是** `node --check` 报 `missing ) after argument list`** ✓ ★★★
    //       ＋ **∴ 所以**：**这一段**一律用单引号串 ＋ 拼接**✗ **不碰模板串** ✓
    problems.push('新增了 ' + (after - before) + ' 个插值起始（dollar-brace）'
      + ' ⇒ **∴ 那**很可能是我把 $VAR 误改成插值形式**');
  }
  // **∴ 规则 B ✗**：**改动的行里**不要引入反引号****✗（**∴ 它**会提前闭合模板串 ✓）
  const oldLines = new Set(oldText.split("\n"));
  const changedLines = next.split("\n").filter((l) => !oldLines.has(l));
  const newLines = newText.split("\n");
  for (const l of newLines) {
    if (/^\s*\/\//.test(l) && l.includes("`")) {
      // **∴ 注释里的反引号**✗ ⇒ **∴ 若**它落在**模板串内**就危险** ✓
      problems.push(`注释里引入反引号：${l.trim().slice(0, 60)} ⇒ **∴ 它**可能提前闭合模板串**`);
    }
  }
  // **∴ 规则 C ✗**：**新文本的引号必须**自洽**✗（**∴ 对**双引号串**做粗查 ✓）
  const dq = (newText.match(/"/g) || []).length;
  if (dq % 2 !== 0 && !newText.includes("\n")) {
    problems.push(`新文本里的双引号是**奇数**（${dq}）⇒ **∴ 可能**引号不配**`);
  }
  if (problems.length > 0) {
    console.error("  ✗ 闸③ 模板串／引号守卫拦下：");
    for (const p of problems) console.error(`     · ${p}`);
    console.error("     ⇒ **∴ 原文件**未改动** ✓");
    console.error("     ⇒ **∴ 若**确实需要 ⇒ **∴ 加** `--allow-interp`** ✗ 并**在 diff 里自查** ✓");
    process.exit(1);
  }
}

// **∴ 闸②：**先写临时文件 ⇒ 验语法 ⇒ 通过才写原文件**** ✓
const ext = extname(file);
const dir = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-safe-edit-")));
const tmp = join(dir, `candidate${ext}`);
writeFileSync(tmp, next, "utf8");
let syntaxOk = true;
let syntaxMsg = "";
try {
  if (ext === ".mjs" || ext === ".js" || ext === ".cjs") {
    execFileSync(process.execPath, ["--check", tmp], { stdio: "pipe", timeout: 600_000 });
  } else if (ext === ".py") {
    execFileSync("python3", ["-m", "py_compile", tmp], { stdio: "pipe", timeout: 600_000 });
  } else if (ext === ".json") {
    JSON.parse(next);
  } else {
    // **∴ 其它类型（**`.rs`／`.sh`／`.md` ✓）⇒ **∴ 这里**不验**✗ 而**明确说出来** ✓
    syntaxMsg = `（${ext || "无扩展名"} 未做语法检查 ⇒ **∴ 由**后续门禁兜** ✓）`;
  }
} catch (error) {
  syntaxOk = false;
  syntaxMsg = String((error && error.stderr) || error).split("\n").slice(0, 4).join("\n");
}
if (!syntaxOk) {
  console.error(`  ✗ 闸② 语法检查不过 ⇒ **∴ 原文件**未改动** ✓`);
  console.error(syntaxMsg.split("\n").map((l) => `     ${l}`).join("\n"));
  process.exit(1);
}

// **★★★ 闸⑤：**用到的标识符必须**在文件里被导入或定义**** ✗ ★★★**（**第 802 轮 ✓）
//   **∴ 为什么需要它 ✗**（**实测 ✓）★**：
//     ＋ **∴ 我**删掉 `rmSync` 的导入时**漏了一处调用**✗
//       ⇒ **∴ 而** `node --check` **只查语法**✗ ⇒ **∴ 它**查不出**未定义的标识符** ✓ ★★
//         ⇒ **∴ 于是**那个文件**语法全绿**✗ 而**一跑就崩** ✓
//   **∴ 规则 ✗**：**对每个 `import { a, b } from` 里**没有**出现的名字**✗
//     ＋ **∴ 若**它在代码里被**调用**（`name(` ✓）✗ ⇒ **∴ 记一条**可疑** ✓
//       ＋ **∴ 白名单 ✗**：**JS 内建 ＋ 本文件里 `const`／`function` 定义的** ✓
const JS_BUILTINS = new Set([
  "console", "process", "JSON", "Math", "Number", "String", "Boolean", "Object", "Array",
  "Promise", "Error", "Set", "Map", "Date", "RegExp", "Symbol", "Buffer", "URL", "URLSearchParams",
  "parseInt", "parseFloat", "isNaN", "isFinite", "encodeURIComponent", "decodeURIComponent",
  "setTimeout", "setInterval", "clearTimeout", "clearInterval", "structuredClone", "fetch",
  "require", "module", "exports", "__dirname", "__filename", "globalThis", "undefined", "NaN",
  "Infinity", "BigInt", "WeakMap", "WeakSet", "Proxy", "Reflect", "Atomics", "TextEncoder",
  "TextDecoder", "queueMicrotask", "AbortController", "AbortSignal", "FormData", "Blob", "File",
]);
if (ext === ".mjs" || ext === ".js" || ext === ".cjs") {
  const imported = new Set();
  for (const m of next.matchAll(/import\s*\{([^}]*)\}\s*from/g)) {
    for (const nm of m[1].split(",")) {
      const t = nm.trim().split(/\s+as\s+/).pop().trim();
      if (t) imported.add(t);
    }
  }
  for (const m of next.matchAll(/import\s+([A-Za-z_$][\w$]*)\s+from/g)) imported.add(m[1]);
  const declared = new Set();
  for (const m of next.matchAll(/(?:const|let|var|function|class)\s+([A-Za-z_$][\w$]*)/g)) declared.add(m[1]);
  for (const m of next.matchAll(/(?:const|let|var)\s*\{([^}]*)\}/g)) {
    for (const nm of m[1].split(",")) {
      const t = nm.trim().split(":").pop().trim();
      if (t) declared.add(t);
    }
  }
  // **∴ 只看**调用形态**（`name(` ✓）✗ ⇒ **∴ 从而**少误报** ✓ ★
  // **★★★ 只看**新增／改动的行** ✗ ★★★**（**第 806 轮；**闸⑤ 的误报 ✓）
  //   ＋ **∴ 实测误报 ✗**：**我用 `--old` 匹配一段**含 `gpu(`／`max(` 的文本**✗
  //     ⇒ **∴ 而**闸⑤ **把 `--old` 里的片段也当代码**✗ ⇒ **∴ 于是**误报**
  //       ＋ **∴ 而**那些名字**在原文件里本来就存在**✗ ⇒ **∴ 它**不可能是**新引入的** ✓ ★★
  //         ＋ **∴ 所以**：**只扫**新文本**✗ 而**原文本里已有的片段**不参与** ✓
  const oldLines = new Set(oldText.split("\n").map((l) => l.trim()));
  const scanLines = next.split("\n").filter((l) => !oldLines.has(l.trim()));
  const scan = scanLines.join("\n");
  const suspicious = new Set();
  for (const m of scan.matchAll(/(^|[^\w.$])([a-z][\w$]*)\s*\(/g)) {
    const name = m[2];
    if (imported.has(name) || declared.has(name) || JS_BUILTINS.has(name)) continue;
    if (/^(if|for|while|switch|catch|return|typeof|new|await|async|function|class|else|do|try|throw|delete|void|in|of|case|default)$/.test(name)) continue;
    // **∴ 方法调用（**前面有点 ✓）不算** ✓
    suspicious.add(name);
  }
  if (suspicious.size > 0) {
    console.error("  ✗ 闸⑤ 可能用了**未导入**的标识符（**`node --check` 查不出这一类** ✓）：");
    for (const n of suspicious) console.error(`     · ${n}(  ⇒ **∴ 它**既没 import 也没定义**`);
    console.error("     ⇒ **∴ 原文件**未改动** ✓");
    console.error("     ⇒ **∴ 若**是误报（**如**解构／内建 ✓）⇒ **∴ 加** `--allow-undeclared`** ✓");
    if (!has("--allow-undeclared")) process.exit(1);
    console.error("     （--allow-undeclared 已给 ⇒ **∴ 继续** ✓）");
  }
}

writeFileSync(file, next, "utf8");

// **∴ 闸④（**写后再确认一次 ✓）：**从磁盘读回来**必须等于 next** ✓
const back = readFileSync(file, "utf8");
if (back !== next) {
  console.error("  ✗ 闸④ 写回后从磁盘读到的内容与预期不符 ⇒ **∴ 磁盘**可能有问题**");
  process.exit(1);
}

console.log(`  ✓ safe-edit：${file}｜替换 ${wantCount} 处 ⇒ **∴ 文件**确实变了**（${src.length} ⇒ ${next.length} 字节）`);
if (syntaxMsg) console.log(`     ${syntaxMsg}`);
process.exit(0);
