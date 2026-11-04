#!/usr/bin/env node
// **判据自己的诚实**：两个检查面（① 第 1082 轮；② 2026-11-04 轮）
//
// **检查面 ①**：用了 `fetch(` 的判据必须有 `process.exit(3)`
//   **∴ 依据（336 轮）**：判据面对「条件不满足」只有三种诚实回应
//     ① 通过（假称查过）② 跳过并说出来 ③ **不同退出码（exit(3)）**
//     ⇒ 而「默认通过」是第四种 ⇒ 看起来像查了 ⇒ **最危险** ✗
//   **∴ 为什么需要**：无服务时 `fetch` **抛异常** ⇒ uncaught ⇒ node 退出码 **1**
//     ⇒ 于是「判据前提不成立」被报成「**产品失败**」✗（本机实测 1079 轮）
//   **∴ 为什么是静态**：真跑一遍不适合作提交门；而「用了 fetch 却没 exit(3)」是
//     **必要不充分**条件 —— 能抓住绝大多数，且**很快** ✓
//
// **检查面 ②**：正文出现「不可判语义」措辞，且**同一段落、向后 300 字符内**出现
//   单独的 `process.exit(0)` 或 `return;` ⇒ 不合格
//   **∴ 为什么要加**：① 只看 `fetch(` ⇒ 有 4 条判据在**一条断言都没跑**时
//     用 `process.exit(0)` 报绿（缺 `sharp`／缺冷启动夹具／缺 `.yanshi` 工程／
//     只有 debug 二进制）⇒ 它们别的分支里写着 `exit(3)` ⇒ ① 抓不到 ✗
//     ⇒ 这四条**已经从 exit(0) 改成 `⊘ 不可判` ＋ exit(3)** ✓
//     ⇒ 本检查面是让这类问题**以后不能再悄悄回来** ✓
//   **∴ 为什么是静态**：真跑要起服务／浏览器，成本高；而「不可判措辞 ＋ exit(0)」
//     正是这一类的**共性形态**，静态就能认 ✓
//
// **∴ 两条纪律**：
//   1. **宁漏不误报**：`exit(0)` 是文件最后一条代码（整份判据已跑完）⇒ 不算假绿；
//      措辞与 `exit(0)` 之间已出现 `process.exit(3)`（配对已存在）⇒ 不算假绿
//   2. **例外必须带理由，且条目必须存在**（过期条目报红 —— 与 check-pwa-local-params.mjs 同规）
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const dir = new URL("../scripts/", import.meta.url).pathname;
const files = readdirSync(dir).filter((f) => /^(tool|mcp)-.*\.mjs$/.test(f)).sort();

let failed = false;

// ===== 检查面 ①：fetch( ⇒ 必须有 process.exit(3)（逻辑未改）=====
const bad = [];
let withFetch = 0;
for (const f of files) {
  const text = readFileSync(join(dir, f), "utf8");
  if (!text.includes("fetch(")) continue;
  withFetch += 1;
  if (!text.includes("process.exit(3)")) bad.push(f);
}

console.log(`  用 fetch( 的判据：${withFetch} 个｜缺 process.exit(3)：${bad.length} 个`);
if (!withFetch) {
  console.error("  ⊘ 不可判：一个用 fetch( 的判据都没扫到（判据前提不成立）");
  process.exit(3);
}
// 只打印前 5 个，避免日志过长
for (const f of bad.slice(0, 5)) console.log(`    ✗ ${f}`);
if (bad.length > 5) console.log(`    …（还有 ${bad.length - 5} 个）`);
if (bad.length) {
  console.log(`  ✗ ${bad.length} 个判据无服务时会把「前提不成立」报成「产品失败」（应为 exit(3)）`);
  failed = true;
} else {
  console.log(`  ✓ ${withFetch} 个用 fetch( 的判据都有 process.exit(3)（前提失败不会被误报成产品失败）`);
}

// ===== 检查面 ②：不可判措辞 ＋ exit(0)／return; ⇒ 假绿 =====
// **∴ 措辞集合**用实词（不用宽泛的 `skip`）⇒ 减少误报 ✓
const UNJUDGEABLE = [
  "跳过",
  "不可判",
  "本次没验",
  "不适用",
  "要真跑",
  "先 npm i",
  "无法运行",
  "判不了",
  "⊘",
];

// **∴ 显式例外**：每条**必须**带一句理由；文件不存在 ⇒ 过期 ⇒ 报红 ✓
const EXCEPTIONS = new Map([
  [
    "tool-pwa-live-smoke.mjs",
    "无网时按成文设计跳过（YANSHI_LIVE_REQUIRE=1 可强制要求联网；断言不符仍 exit 1）",
  ],
  [
    "cdp-seed-client-render.mjs",
    "CDP 注入辅助脚本：没有断言，不是判据（它跳过是为了不拖累调用它的判据）",
  ],
]);
const expired = [...EXCEPTIONS.keys()].filter((f) => !existsSync(join(dir, f)));
console.log(
  `  不可判措辞 ＋ exit(0) 扫描｜例外清单：${EXCEPTIONS.size} 条（每条带理由）｜过期：${expired.length} 条`,
);
if (expired.length) {
  for (const f of expired) console.log(`    ✗ 例外条目指向的文件不存在（过期）：${f}`);
  failed = true;
}

const allFiles = readdirSync(dir)
  .filter((f) => f.endsWith(".mjs"))
  .sort();
if (!allFiles.length) {
  console.error("  ⊘ 不可判：scripts/ 里一个 .mjs 都没扫到（判据前提不成立）");
  process.exit(3);
}

// 「不可判措辞 → 单独的 exit(0)／return;」的命中；两种已确认合法的情况分别计数
const hits = [];
let hitsExit3 = 0; // 措辞与 exit(0) 之间已有 exit(3) ⇒ 配对已存在
let hitsTail = 0; // exit(0) 是最后一条代码**且**之前跑过断言 ⇒ 整份判据已跑完

for (const f of allFiles) {
  if (EXCEPTIONS.has(f)) continue;
  const src = readFileSync(join(dir, f), "utf8");
  const lines = src.split("\n");

  // 每行的起始偏移（用于把「行」换算成「字符位置」）
  const starts = [];
  let offset = 0;
  for (const line of lines) {
    starts.push(offset);
    offset += line.length + 1;
  }

  // 只认**单独成行**的 exit(0)／return; ⇒ 注释里的 `process.exit(0)` 不会被当成代码 ✓
  const exits = [];
  lines.forEach((line, i) => {
    if (/^\s*process\.exit\(0\)\s*;?\s*$/.test(line)) {
      exits.push({ pos: starts[i], line: i, kind: "process.exit(0)" });
    } else if (/^\s*return;\s*$/.test(line)) {
      exits.push({ pos: starts[i], line: i, kind: "return;" });
    }
  });
  if (!exits.length) continue;

  lines.forEach((line, i) => {
    const trimmed = line.trim();
    // 纯注释行里的措辞不算（正文里的说明不产生行为）
    if (trimmed.startsWith("//") || trimmed.startsWith("*") || trimmed.startsWith("/*")) return;
    for (const keyword of UNJUDGEABLE) {
      for (let at = line.indexOf(keyword); at >= 0; at = line.indexOf(keyword, at + 1)) {
        const kwPos = starts[i] + at;
        // 同一段落（不跨空行）且向后 300 字符内
        const paragraphEnd = (() => {
          for (let j = i + 1; j < lines.length; j++) {
            if (lines[j].trim() === "") return starts[j];
          }
          return src.length;
        })();
        const windowEnd = Math.min(kwPos + 300, paragraphEnd);
        const exit = exits.find((e) => e.pos >= kwPos && e.pos < windowEnd);
        if (!exit) continue;

        const between = src.slice(kwPos, exit.pos);
        if (between.includes("process.exit(3)")) {
          hitsExit3 += 1;
          continue;
        }
        // exit 之后是否只剩空白与收尾括号（= 文件最后一条代码），**并且**这之前真的跑过断言
        // ⇒ 只看「在最后」不够：一个只有「打印不可判 ＋ exit(0)」的文件也在最后 ✗
        const newlineAt = src.indexOf("\n", exit.pos);
        const tail = newlineAt < 0 ? "" : src.slice(newlineAt + 1);
        const tailIsCode = tail
          .split("\n")
          .map((l) => l.trim())
          .filter((l) => l !== "" && !l.startsWith("//") && !l.startsWith("*"));
        const ranAssertion = /\b(?:check|assert|expect)\s*\(|failures\.push\s*\(/.test(
          src.slice(0, exit.pos),
        );
        if (ranAssertion && tailIsCode.every((l) => /^[\s});,\]]*$/.test(l))) {
          hitsTail += 1;
          continue;
        }
        hits.push({ file: f, line: i + 1, keyword, exitKind: exit.kind, exitLine: exit.line + 1 });
      }
    }
  });
}

// 同一行关键词命中同一处 exit 会重复（例如「不适用」与「跳过」同行）⇒ 去重
const unique = [];
const seen = new Set();
for (const h of hits) {
  const key = `${h.file}:${h.line}:${h.exitLine}`;
  if (seen.has(key)) continue;
  seen.add(key);
  unique.push(h);
}

const hitFileCount = new Set(unique.map((h) => h.file)).size;
console.log(
  `  扫 ${allFiles.length} 个 .mjs｜假绿：${hitFileCount} 个文件／${unique.length} 处｜已配对 exit(3)：${hitsExit3} 处｜尾部通过：${hitsTail} 处`,
);
for (const h of unique.slice(0, 10)) {
  console.log(`    ✗ ${h.file}:${h.line} 「${h.keyword}」后接 ${h.exitKind}（第 ${h.exitLine} 行）`);
}
if (unique.length > 10) console.log(`    …（还有 ${unique.length - 10} 个）`);
if (unique.length) {
  console.log(
    `  ✗ ${hitFileCount} 个判据在「不可判」时用 exit(0) 报绿（应为 ⊘ 不可判 ＋ process.exit(3)）`,
  );
  failed = true;
} else {
  console.log("  ✓ 没有判据在「不可判」时报绿（不可判措辞都配 exit(3)，或只是局部跳过）");
}

if (failed) process.exit(1);
