#!/usr/bin/env node
// **`BAND_START` 探针的实参顺序必须与占位符一致**（第 746 轮；外部审计 R-12）
//
// **⇒ 缺陷**：`t_ns={}` 拿到的是 `region` 的四个数字之一，而 `region=(…)` 拿到的是**纳秒时间戳**
//   ✗ ⇒ 探针**看上去正常**（都是数字 ✓）而**含义全错** ✓ —— 诊断输出错得最坏的一种：
//   **它会把人带偏** ✓（本仓「我的测量比被测对象更容易错」的第 N 次 ✓）。
//
// **⇒ 判据**：从源码里取出那条 `format!` 的**占位符序列**与**实参序列**，断言：
//   `t_ns` 那个位置对应的是**时间戳表达式**（`duration_since` ✓），
//   `region` 那四个位置对应的是 **`rx/ry/rw/rh`** ✓。
// **⇒ 变异**：把时间戳挪回最后 ⇒ 红 ✓。
//
// **⇒ 为什么是结构检查** ✗：跑真探针要设 `YANSHI_*` 环境变量并解析文件 ✓，
//   而**测试进程共享环境变量**（并行不安全 ✓）⇒ 退一步用源码比对，文案里说清它只证明顺序 ✓。
import { readFileSync } from "node:fs";

const src = readFileSync("crates/yanshi-render/src/render.rs", "utf8");
const at = src.indexOf('"BAND_START thread={:?} y={} rows={} t_ns={} region=({},{},{},{})');
if (at < 0) {
  console.error("  ⊘ 不可判：找不到 `BAND_START` 的格式串（探针改名了？）");
  process.exit(3);
}
// 取 format! 的实参段：从格式串之后到匹配的 `)` 结束（按括号配平）
const open = src.indexOf("(", src.indexOf("format!", at - 200));
let depth = 0;
let index = open;
let end = open;
while (index < src.length) {
  if (src[index] === "(") depth += 1;
  else if (src[index] === ")") {
    depth -= 1;
    if (depth === 0) {
      end = index;
      break;
    }
  }
  index += 1;
}
const body = src.slice(open, end);
// 去掉格式串本身与注释
const code = body
  .replace(/"(?:[^"\\]|\\.)*"/g, "")
  .split("\n")
  .map((line) => line.replace(/\/\/.*$/, ""))
  .join("\n");
// **第一个元素是 `format!(` 自己的左括号** ✓ ⇒ 丢掉它 ✓（否则整列错位一格 ✓ ——
//   我第一次就是这么错的：把时间戳当成了 `rx`，判据直接报"region 第 1 位不是 rx"）。
const args = code
  .split(",")
  .map((part) => part.trim())
  .filter((part) => part.length > 0 && part !== "(");

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label);
};

// 占位符顺序：thread, y, rows, t_ns, region×4 ⇒ 实参 9 个（第 4 个是时间戳，5..8 是 rx/ry/rw/rh）
check(args.length === 8, "实参共 8 个（thread/y/rows/t_ns ＋ region 四个）", `实测 ${args.length}`);
check(/duration_since/.test(args[3] || ""), "第 4 个实参（`t_ns`）是时间戳", args[3]);
for (const [offset, name] of ["rx", "ry", "rw", "rh"].entries()) {
  const arg = args[4 + offset] || "";
  check(arg.includes(name), `第 ${5 + offset} 个实参（region 第 ${offset + 1} 位）是 \`${name}\``, arg);
}

if (failures.length) {
  console.error(`  ✗ ${failures.length} 项没成立 ⇒ 探针会把时间戳当坐标（外部审计 R-12）`);
  process.exit(1);
}
console.log("✓ BAND_START 的实参顺序与占位符一致（结构检查：只证明顺序）");
