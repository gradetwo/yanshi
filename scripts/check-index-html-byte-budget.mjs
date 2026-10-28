#!/usr/bin/env node
// **`web/index.html` 的字节预算：只许降、不许悄悄涨**（05 复审 NEW-H1 的防回归半边）
//
// **⇒ 背景**：审计点出查看器单文件 1.04 MB ✗（NEW-H1）。瘦身是大手术（面板拆分/构建管线），
//   排期在后 ⇒ 但**排期之前先止损**：不许每次合并补丁顺手再塞进去几十 KB 而没人看见 ✓。
//
// **⇒ 为什么可以写死这个数**（判据两分类 ✓）：CEILING 是**判据自己造的期望值**
//   （棘轮：越改只许越小），不是被测系统的数量 ⇒ 不属「上游一加就失效」那一类。
//   当前实测值用命令现算（`wc -c web/index.html`），注释里带时间戳（内核 ② ✓）。
//
// **⇒ 判什么**：① 文件读得到；② 字节数 ≤ CEILING；③ 若已逼近（≥95%）打印警示但不红
//   （棘轮的含义是**拦住增长**，不是拦住逼近）。
// **⇒ 变异**：往 index.html 追加 12 KB ⇒ ② 红 ✓（实测红过再回绿 ✓）。
// **⇒ 红之后怎么办**：若增长是有意的（合并补丁带进新功能），把 CEILING 改成新值、
//   在提交信息里写清「谁涨了多少、为什么」——让膨胀**必须被看见**，这就是本判据的全部目的。
import { statSync, readFileSync } from "node:fs";

// 棘轮上界（截至 2026-10-28 实测 1,038,359 字节 ⇒ 上界取 1,040,000 留一点余量）。
// 数它用 `wc -c web/index.html`；改这个数必须附理由。
const CEILING = 1_040_000;

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

let size;
try {
  size = statSync("web/index.html").size;
  readFileSync("web/index.html");
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 web/index.html（不是产品问题）");
  process.exit(3);
}

const ratio = size / CEILING;
check(ratio <= 1, `index.html ${size.toLocaleString("en")} 字节 ≤ 预算 ${CEILING.toLocaleString("en")}`,
  ratio > 1 ? `超出 ${(size - CEILING).toLocaleString("en")} 字节 ⇒ 要么瘦身，要么带着理由抬棘轮` : `用了 ${(ratio * 100).toFixed(1)}%`);
if (ratio <= 1 && ratio >= 0.95) {
  console.log(`  ⚠ 已用 ${(ratio * 100).toFixed(1)}% 预算 ⇒ 下次合并前先考虑瘦身（NEW-H1）`);
}

if (failures.length > 0) {
  console.error(`✗ 字节预算判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log("✓ web/index.html 在字节预算内");
