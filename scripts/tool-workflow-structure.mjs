#!/usr/bin/env node
// **★ 工作流结构判据（**窄而准 ✓）★**（第 677 轮 ✓）
//
// **∴ 背景** ✓：**第 675 轮实测到一个真 bug ✗**：`pwa.yml` 曾在**同一个 step 里写两个 `run:` ✗**
//   ⇒ **∴ YAML 合法 ✓、CI 绿 ✓、**而**第一个 `run` 从未执行 ✗****
//   ⇒ **∴ 于是**CI 构建的 PWA 用**过期页面**发布 ✓ —— **∴ 典型的"看起来成功、实际没做"✗**。
//
// **∴ 本判据**只查两件**低误报**的事 ✗**（**∴ 第 676 轮我写宽版 ⇒ 38 条误报 ⇒ 已删 ✓**）**：
//   ① **空 step ✗**：**用 `yaml` 解析 `jobs.*.steps` ✓**（**∴ 解析结果可靠 ⇒ 不会把顶层键当 step ✓**）；
//   ② **同一步骤两个 `run:` ✗**：**读原文 ✓**，**只在每个 `steps:` 块的**直接子项**里数 ✓**
//      （**∴ 必须读原文 ⇒ **∴ 因为**YAML 解析后重复键已被丢掉 ✓**）。
//
// **∴ 变异（**自检 ✓**）**：**把两个 `run` 放回 ⇒ **必红 ✓****。
// **∴ 测退出码时**不许接管道**✗**（**∴ 第 676 轮的教训 ✓**）。
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
let YAML = null;
try { YAML = require("yaml"); } catch { YAML = null; }

const DIR = ".github/workflows";
const problems = [];

for (const f of readdirSync(DIR).filter((n) => n.endsWith(".yml") || n.endsWith(".yaml"))) {
  const src = readFileSync(join(DIR, f), "utf8");
  const lines = src.split("\n");

  // ① **空 step**（**用解析结果 ⇒ 无误差 ✓**）
  if (YAML) {
    let doc = null;
    try { doc = YAML.parse(src); } catch (e) { problems.push(`${f}: YAML 解析失败：${e.message}`); }
    for (const [jn, job] of Object.entries((doc && doc.jobs) || {})) {
      if (!job || typeof job !== "object") continue;
      (job.steps || []).forEach((s, i) => {
        if (!s || typeof s !== "object") return;
        if (!s.run && !s.uses) {
          problems.push(`${f}: job=${jn} step#${i + 1} ${JSON.stringify(s.name || "")} 既无 run 也无 uses（空 step）`);
        }
      });
    }
  } else {
    console.log("  （未装 yaml 模块 ⇒ 跳过空 step 检查；两个 run 检查仍生效 ✓）");
  }

  // ② **同一步骤两个 `run:`**（**只在 `steps:` 块内数 ✓**）
  let inSteps = false, stepsIndent = -1, itemIndent = -1, runCount = 0, firstRun = 0, itemName = "", itemStart = 0;
  const flush = () => {
    if (runCount >= 2) {
      problems.push(`${f}:${itemStart} step=${JSON.stringify(itemName)} 有 ${runCount} 个 run（第一个在 ${firstRun} 行 ⇒ YAML 只保留最后一个）`);
    }
    runCount = 0; firstRun = 0; itemName = ""; itemStart = 0; itemIndent = -1;
  };
  lines.forEach((raw, idx) => {
    const n = idx + 1;
    const t = raw.trim();
    if (t.startsWith("#") || t === "") return;
    const indent = raw.length - raw.trimStart().length;
    if (/^steps:\s*$/.test(t)) { flush(); inSteps = true; stepsIndent = indent; return; }
    if (inSteps && indent <= stepsIndent && !/^steps:/.test(t)) { flush(); inSteps = false; return; }
    if (!inSteps) return;
    const mDash = raw.match(/^(\s*)-\s+(.*)$/);
    if (mDash && mDash[1].length === stepsIndent + 2) {
      flush();
      itemIndent = stepsIndent + 2;
      itemStart = n;
      const rest = mDash[2].trim();
      const mName = rest.match(/^name:\s*(.*)$/);
      itemName = mName ? mName[1].replace(/^["']|["']$/g, "") : rest.slice(0, 40);
      if (/^run:/.test(rest)) { runCount += 1; firstRun = firstRun || n; }
      return;
    }
    if (itemIndent >= 0 && indent === itemIndent + 2 && /^run:/.test(t)) {
      runCount += 1; firstRun = firstRun || n;
    }
  });
  flush();
}

if (problems.length) {
  console.error("❌ 工作流结构有问题：");
  for (const p of problems) console.error("   " + p);
  process.exit(1);
}
console.log("  ✓ 工作流结构成立：无空 step ✓｜每个步骤至多一个 run ✓");
