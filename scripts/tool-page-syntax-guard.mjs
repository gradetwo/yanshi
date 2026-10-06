#!/usr/bin/env node
// **判据脚本的"页面侧语法"守卫** ✓（2026-10-06 ✓）。
//
// **为什么需要** ✓：`evaluate(` 后面跟反引号时 ⇒ **模板体是数据** ✗ ⇒ 它里面的语法**要等运行时**
// 才由浏览器（或 CDP 的目标）解析 ✓ ⇒ 于是"注释里写了反引号"这类错误会让**模板提前终止** ✗
// ⇒ 而 `node --check` 只能看到**外层**变成了非法 JS ⇒ 有时能报、有时报在不相关的行 ✓
// ⇒ **判据完全跑不了** ✗（比没有诊断更糟 ✓）。
//
// **做法（能红 ✓）** ✓：把每个 `evaluate` 的模板体取出 ⇒ 用 `new Function(体)` **只解析不执行** ✓
// ⇒ 语法错就抛 ⇒ 我们把它报成红 ✓。这正是 `browser-ui-check.mjs` 自己的注释（`:443` ✓）写的招 ✓。
//
// **为什么不用"找裸反引号"✗** ✓：我第一版那么写 ⇒ 把"模板的第一个反引号"当成正常结束 ⇒
// 于是**恰恰放过要抓的那个反引号** ✗ ⇒ 变异测试**不变红** ⇒ 那是**空转的守卫** ✓（已撤 ✓）。
import { readFileSync } from "node:fs";

const files = process.argv.slice(2);
if (files.length === 0) { console.error("用法: node scripts/tool-page-syntax-guard.mjs <file...>"); process.exit(2); }

let bad = 0;
for (const file of files) {
  const text = readFileSync(file, "utf8");
  const re = /evaluate\(`/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const start = m.index + m[0].length;
    let i = start;
    while (i < text.length) {
      if (text[i] === "`" && text[i - 1] !== "\\") break;
      i += 1;
    }
    const body = text.slice(start, i);
    try {
      new Function(body);                     // **只解析 ✓，不执行 ✓**
    } catch (error) {
      const line = text.slice(0, start).split("\n").length;
      console.log(`  ✗ ${file}:${line} evaluate 的模板体不是合法 JS ⇒ ${String(error.message).slice(0, 90)}`);
      bad += 1;
    }
  }
}
if (bad > 0) { console.log(`  共 ${bad} 处 ⇒ 判据一跑就会抛错 ✗`); process.exit(1); }
console.log("  ✓ evaluate 的模板体都是合法 JS ✓");
