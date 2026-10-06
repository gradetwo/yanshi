#!/usr/bin/env node
// **模板字面量守卫** ✓：`evaluate(` 后面跟反引号时，那个模板**体内不能有裸反引号** ✗。
//
// **为什么需要它** ✓：`node --check` **看不到模板体内**的语法 ✗ —— 模板是**运行时**才解析的 ✓
// ⇒ 于是"注释里写了反引号"这类错误 ⇒ `node --check` 报 OK ✓ ⇒ 而运行整条判据时抛
// `SyntaxError: missing ) after argument list` ✗ ⇒ **判据完全跑不了** ✓（比没有诊断更糟 ✓）。
//
// **实测（2026-10-06 ✓）**：我在同**一次**改动里因此弄坏 `browser-ui-check.mjs` 两次 ✓，
// 而且第二次是**在注释里写着"注释里不能出现反引号"的那一行**上犯的 ✓。
//
// **做法** ✓：找出每个 `evaluate(` 之后的模板体（配对的结束反引号 ✓），断言体内没有裸反引号 ✓。
// **不用"逐行猜是否在模板内"✗** —— 我第一版那么写 ⇒ 误报 79 处 ✓（无关注释被当成模板内 ✓）。
import { readFileSync } from "node:fs";

const files = process.argv.slice(2);
if (files.length === 0) { console.error("用法: node scripts/tool-template-literal-guard.mjs <file...>"); process.exit(2); }

let bad = 0;
for (const file of files) {
  const text = readFileSync(file, "utf8");
  const re = /evaluate\(`/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const start = m.index + m[0].length;      // exec 返回数组 ⇒ 没有 .end() ✓
    let i = start;
    while (i < text.length) {
      if (text[i] === "`" && text[i - 1] !== "\\") break;
      i += 1;
    }
    const body = text.slice(start, i);
    const naked = body.replace(/\\`/g, "");
    if (naked.includes("`")) {
      const line = text.slice(0, start + naked.indexOf("`")).split("\n").length;
      console.log(`  ✗ ${file}:${line} evaluate 的模板体内有裸反引号 ⇒ 会让模板提前终止（运行时才报 ✗）`);
      bad += 1;
    }
  }
}
if (bad > 0) { console.log(`  共 ${bad} 处 ⇒ 判据会在运行时抛 SyntaxError ✗`); process.exit(1); }
console.log("  ✓ evaluate 模板体内没有裸反引号 ✓");
