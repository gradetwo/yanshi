#!/usr/bin/env node
// **Makefile 的两条 hygiene 不许悄悄退化**（05 复审 L-2／L-3）
//
// **⇒ 背景**：L-2 ✗ — `.PHONY` 漏了 `serve`／`targets` ⇒ 哪天目录里出现同名文件
//   ⇒ `make serve` 会**静默跳过**（看起来成功 ✓ 实际什么都没跑 ✗）。
//   L-3 ✗ — `clean` 写死 `rm -rf dist` 而入口明明有 `OUT ?= dist` ⇒ `make OUT=… clean`
//   **清错地方**（实测：`make -n clean OUT=/tmp/x` 原来打印 `rm -rf dist` ✗）。
//
// **⇒ 判什么**：① Makefile 里声明的每个**冒泡目标**都要出现在 `.PHONY` ✓；
//   ② `clean` 的配方必须用 `$(OUT)` 而不是写死 `dist` ✓。
//   ① 是**开集**断言（以后新增目标忘了 .PHONY 会当场红 ✓），不是快照数字 ✓。
//
// **⇒ 变异**：把 `.PHONY` 里的 `serve` 删掉 ⇒ ① 红 ✓；把 clean 换回 `rm -rf dist` ⇒ ② 红 ✓。
import { readFileSync } from "node:fs";

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label + (detail ? `：${detail}` : ""));
};

let text;
try {
  text = readFileSync("Makefile", "utf8");
} catch {
  console.error("  ⊘ 判据前提不成立：读不到 Makefile（不是产品问题）");
  process.exit(3);
}

const lines = text.split("\n");

// 取 .PHONY 行（可能跨行反斜杠续行，逐行拼）。
let phony = "";
for (let i = 0; i < lines.length; i++) {
  if (lines[i].startsWith(".PHONY:")) {
    let j = i;
    while (j < lines.length) {
      phony += " " + lines[j].replace(/^\.PHONY:/, "").replace(/\\\s*$/, "");
      if (!lines[j].endsWith("\\")) break;
      j++;
    }
    break;
  }
}
if (!phony.trim()) {
  console.error("  ⊘ 判据前提不成立：Makefile 里没有 .PHONY 声明（不是产品问题）");
  process.exit(3);
}
const phonySet = new Set(phony.trim().split(/\s+/));

// 收集所有形如 `name:`（或 `name: 依赖`）的目标行；跳过变量赋值、注释、以 . 开头的特殊行。
const targets = [];
const reTarget = /^([A-Za-z0-9_-]+):(?:\s|$)/;
for (const line of lines) {
  if (line.startsWith("#") || /^[.]|^[A-Za-z0-9_-]+\s*[?+]?=/.test(line)) continue;
  const m = line.match(reTarget);
  if (m && m[1] !== "Makefile") targets.push(m[1]);
}
// 集合为空 ⇒ 判据没在验东西（四层齐之「输入能触发」）。
check(targets.length > 0, "能收集到目标清单", `实测 ${targets.length} 个`);

const missing = targets.filter((t) => !phonySet.has(t));
check(missing.length === 0, "每个目标都声明了 .PHONY", missing.length ? `缺 ${missing.join("、")}` : "");

// clean 配方必须认 $(OUT)。
const cleanIdx = lines.findIndex((l) => /^clean:(?:\s|$)/.test(l));
const recipe = cleanIdx < 0 ? [] : lines.slice(cleanIdx + 1).filter((l) => /^\t/.test(l));
check(cleanIdx >= 0 && recipe.length > 0, "clean 目标存在且有配方", `配方 ${recipe.length} 行`);
check(
  recipe.some((l) => l.includes("$(OUT)")),
  "clean 用 $(OUT) 而非写死目录",
  recipe.join(" ; ")
);
check(
  !recipe.some((l) => /rm\s+-rf\s+dist\b/.test(l)),
  "clean 配方里没有写死的 rm -rf dist",
  ""
);

if (failures.length) {
  console.log(`✗ makefile-hygiene：${failures.length} 条不过`);
  process.exit(1);
}
console.log(`✓ makefile-hygiene：${targets.length} 个目标全部 .PHONY，clean 认 OUT=`);
