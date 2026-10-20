#!/usr/bin/env node
// **凡是写了「实测性能数字」的文档，必须指向唯一权威 `docs/performance-notes.md`**（第 1391 轮）
//
// **⇒ 为什么有这条** ✗：我在 `performance-notes.md` 里宣称「数字只写这里」⇒
//   而实测（第 1391 轮）：同一批数字**出现在 2–5 份文档里**
//   （`2 581` 在 2 份、`177 ms` 在 3 份、`4.5` 在 5 份、`0.17 s` 在 3 份）✗
//   ⇒ **⇒ 所以「不重复」做不到（对外信件与台账都需要内联数字）⇒ 那就守住**指针**：**
//     **每个带性能数字的文档都必须写明「权威在 performance-notes.md」** ✓
//
// **⇒ 断什么**：
//   ① `docs/performance-notes.md` 存在，且至少有 5 个 `## ` 小节
//   ② 每个含「性能数字特征」的 `docs/*.md` 都要提到 `performance-notes.md`
//   ③ 打印「命中了几份文档」⇒ 数字会变 ⇒ **不写死** ✓
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync, readdirSync } from "node:fs";

const DOCS = "docs";
const AUTH = "performance-notes.md";
// **性能数字的特征**（出现这些 ⇒ 这份文档在讲实测成本）
const MARKERS = [/ms\/笔/, /[0-9]+\s*ms（/, /[0-9.]+\s*s\/笔/, /[0-9]+\s*ms 的/];

let files;
try {
  files = readdirSync(DOCS).filter((n) => n.endsWith(".md"));
} catch {
  console.error("  ⊘ 不可判：读不到 docs/");
  process.exit(3);
}

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① 权威文档在，且有足够的小节
let authText = null;
try {
  authText = readFileSync(`${DOCS}/${AUTH}`, "utf8");
} catch {
  bad(`① 权威文档 ${DOCS}/${AUTH} 不存在`);
}
if (authText) {
  const sections = (authText.match(/^## /gm) || []).length;
  if (sections >= 5) ok(`① ${AUTH} 有 ${sections} 个 ## 小节`);
  else bad(`① ${AUTH} 至少应有 5 个 ## 小节 ⇒ 实测 ${sections}`);
}

// ② 带性能数字的文档必须指向权威
const carriers = [];
const missing = [];
for (const name of files) {
  if (name === AUTH) continue;
  const text = readFileSync(`${DOCS}/${name}`, "utf8");
  const hasNumber = MARKERS.some((re) => re.test(text));
  if (!hasNumber) continue;
  carriers.push(name);
  if (!text.includes(AUTH) && !text.includes("performance-notes")) missing.push(name);
}
if (missing.length === 0) ok(`② 带性能数字的 ${carriers.length} 份文档全部指向 ${AUTH}`);
else bad(`② 这些文档有性能数字却没指向 ${AUTH}：${missing.join("、")}`);

// ③ 打印（数字会变 ⇒ 不写死）
console.log(`  ③ 打印：带性能数字的文档 ${carriers.length} 份（${carriers.join("、")}）`);

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（性能数字必须指向唯一权威）`);
  process.exit(1);
}
console.log("\n  ✓ 性能数字的指针齐全");
