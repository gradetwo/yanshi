#!/usr/bin/env node
// **每个 browser-* 判据必须自述口径**（05 复审 NEW-M2 的守卫）
//
// **⇒ 背景**：`run-criteria.sh` 曾经用一段硬编码 `case` 决定给哪些 `browser-*.mjs`
//   注入 `render=client` ✗ ⇒ 每加一条判据就要来改运行器 ✓ 口径散落在别人家里 ✗。
//   现在改成**判据自述**：脚本顶部一行 `// criterion-target: client|server|static|self` ✓，
//   运行器只 grep 这一行 ✓（声明 server／static／self ⇒ 不注入 ✓）。
//
// **⇒ 判什么**（三条，都是**开集**断言，新判据漏声明会当场红 ✓）：
//   ① 每个 `scripts/browser-*.mjs` 都有**恰好一条** `^// criterion-target:` ✓；
//   ② 声明值只许 `client|server|static|self` ✓；
//   ③ 运行器里**不许再有**按脚本名硬编码的例外名单 ✓（守 NEW-M2 的根：口径归判据自己）。
//
// **⇒ 变异**：删掉某个脚本的声明行 ⇒ ① 红 ✓；把值改成 `gpu` ⇒ ② 红 ✓；
//   往 run-criteria.sh 塞回 `browser-i18n.mjs)` 名单 ⇒ ③ 红 ✓。
import { readdirSync, readFileSync } from "node:fs";

const VALID = new Set(["client", "server", "static", "self"]);
const failures = [];
const check = (ok, label, detail) => {
  if (!ok) {
    console.log(`  ✗ ${label}${detail ? `（${detail}）` : ""}`);
    failures.push(label);
  }
};

let files;
try {
  files = readdirSync("scripts").filter((f) => /^browser-.*\.mjs$/.test(f)).sort();
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 scripts/（不是产品问题）");
  process.exit(3);
}
// 集合为空 ⇒ 判据没在验任何东西（四层齐之「输入能触发」）。
if (files.length === 0) {
  console.error("  ⊘ 判据前提不成立：scripts/ 下没有 browser-*.mjs（不是产品问题）");
  process.exit(3);
}

const undeclared = [];
const badValue = [];
const doubled = [];
for (const f of files) {
  const lines = readFileSync(`scripts/${f}`, "utf8").split("\n");
  const hits = lines
    .map((l, i) => [i, l])
    .filter(([, l]) => /^\/\/ criterion-target:/.test(l));
  if (hits.length === 0) undeclared.push(f);
  else if (hits.length > 1) doubled.push(f);
  else {
    const val = hits[0][1].match(/^\/\/ criterion-target:\s*(\S+)/)?.[1];
    if (!VALID.has(val)) badValue.push(`${f}: "${val}"`);
  }
}
check(undeclared.length === 0, "每个 browser-* 都声明了 criterion-target", undeclared.join("、"));
check(doubled.length === 0, "声明恰好一条（多条会让运行器的 grep 结果不确定）", doubled.join("、"));
check(badValue.length === 0, "声明值在 client|server|static|self 之内", badValue.join("、"));

// ③ 运行器不许按脚本名硬编码口径 ⇒ 判据：browser 分支里不得出现「多个脚本名用 | 并列的 case 名单」。
const runner = readFileSync("scripts/run-criteria.sh", "utf8");
const nameList = runner.match(/browser-[a-z0-9-]+\.mjs\|browser-/);
check(!nameList, "run-criteria.sh 里没有按名字硬编码的例外名单", nameList ? `发现 ${nameList[0]}…` : "");

const counts = { client: 0, server: 0, static: 0, self: 0 };
for (const f of files) {
  const m = readFileSync(`scripts/${f}`, "utf8").match(/^\/\/ criterion-target:\s*(\S+)/m);
  if (m && m[1] in counts) counts[m[1]]++;
}
if (failures.length === 0) {
  console.log(`✓ browser-criteria-target：${files.length} 条判据全部自述口径（client ${counts.client}／server ${counts.server}／static ${counts.static}／self ${counts.self}）`);
} else {
  console.log(`✗ browser-criteria-target：${failures.length} 条不过`);
  process.exit(1);
}
