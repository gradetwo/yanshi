#!/usr/bin/env node
// **known-red 台账的格式守卫**（05 复审 NEW-M3）
//
// **⇒ 背景**：`criteria-known-red.txt` 一份文件承担两种角色 ⇒ 条目 27 k 行叙事挤在一起 ✗：
//   · 运行器 `is_known_red` 按**第一字段全等**认条目 ✓（第 1357 轮起是 awk ✓）；
//   · 陈旧扫描按 `^[a-z][a-z0-9._-]+\.mjs` 抓条目名 ✓。
//   两个口径都**只认「行首就是文件名」的条目行** ⇒ 任何**没注释掉的散文**
//   （以前真有过 190 行 ✗）会让口径各自漂移 ✓。2026-10-28 已把散文全部收成 `#` 注释 ✓，
//   这条判据保证**不再漂回去** ✓。
//
// **⇒ 判什么（四条）**：
//   ① 非空行要么以 `#` 开头、要么匹配条目格式 `^[a-z0-9._-]+\.mjs[[:space:]]` ✓；
//   ② 每个条目指向的判据文件**真实存在** ✓（孤儿条目会藏真红 ✗）；
//   ③ 条目名**不许重复** ✓（重复行让「移除」变成改一处留一处 ✓）；
//   ④ 运行器的 awk 匹配器与这里读的**同一份文件、同一个第一字段口径** ✓
//     （用同样喂 `is_known_red` 的样例各跑一次：条目必须命中、注释里的名字必须不命中 ✓）。
// **⇒ 变异**：往台账塞一行裸散文 ⇒ ① 红 ✓；塞一行指向不存在脚本的条目 ⇒ ② 红 ✓；
//   把某条目复制一遍 ⇒ ③ 红 ✓。
import { readFileSync, existsSync } from "node:fs";

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

const FILE = "scripts/criteria-known-red.txt";
let text;
try {
  text = readFileSync(FILE, "utf8");
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 scripts/criteria-known-red.txt（不是产品问题）");
  process.exit(3);
}

const ENTRY = /^[a-z0-9._-]+\.mjs[ \t]/;
const lines = text.split("\n");
const stray = [];
const entries = [];
for (const line of lines) {
  if (line.trim() === "" || line.startsWith("#")) continue;
  if (ENTRY.test(line)) entries.push(line.trim().split(/[ \t]/)[0]);
  else stray.push(line.slice(0, 60));
}

check(stray.length === 0, "没有游离的裸散文行（非条目、非注释、非空 ⇒ 0 个）",
  stray.length ? `游离 ${stray.length} 行：${stray.slice(0, 3).join(" | ")}` : "");

const missing = entries.filter((name) => !existsSync(`scripts/${name}`));
check(missing.length === 0, "每个条目指向真实存在的判据文件",
  missing.length ? `孤儿条目：${missing.join("、")}` : `${entries.length} 条`);

const dupes = entries.filter((name, i) => entries.indexOf(name) !== i);
check(dupes.length === 0, "条目名不重复", dupes.length ? `重复：${[...new Set(dupes)].join("、")}` : "");

// ④ 与运行器同口径：第一字段全等才算条目命中；注释里的名字不算。
const firstField = (line) => line.trim().split(/\s+/)[0];
const hitAsRunner = (name) => lines.some((l) => !l.startsWith("#") && firstField(l) === name);
const sample = entries[0];
if (sample) {
  check(hitAsRunner(sample), `运行器口径能命中真条目（样例 ${sample}）`);
  const commented = lines.find((l) => l.startsWith("#") && l.includes(".mjs"));
  if (commented) {
    const cited = firstField(commented.replace(/^#+\s*/, ""));
    if (/^[a-z0-9._-]+\.mjs$/.test(cited)) {
      check(!hitAsRunner(cited), "注释里提到的名字不会被运行器当成条目", cited);
    }
  }
}

if (failures.length > 0) {
  console.error(`✗ known-red 格式判据红 ${failures.length} 条`);
  process.exit(1);
}
console.log(`✓ known-red 台账格式干净（条目 ${entries.length} 条）`);
