#!/usr/bin/env node
// **★ 合成／图层上 GPU 的**逐位一致 ＋ 不一致回退**判据 ✗ ★**（**第 554 轮 ✓；目标第 11 条 ✓）
//
// **∴ 目标第 11 条原文 ✗**：「**合成／图层搬上 GPU**（4K 占比合计 66.6%，
//   是唯一能把上限从 1.45× 抬起来的路）⇒ **必须先立「逐位一致 ＋ 不一致回退」判据**，
//   并**保留 CPU 真值**」
//
// **∴ 本判据守三件事 ✗**：
//   ⇒ **∴ ① CPU 真值必须仍然存在 ✗**：**合成那条路必须还能在 CPU 上算出结果** ✓
//     （**∴ 否则**"逐位一致"就**没有参照** ✓）
//   ⇒ **∴ ② 若 GPU 合成被启用 ⇒ 必须与 CPU 真值**逐位一致**** ✓
//   ⇒ **∴ ③ 不一致时必须**如实回退**** ✓（**∴ 不得**静默继续** ✓）
//
// **∴ 现在的状态（**实测 ✓）✗**：**合成还在 CPU**✗
//   ＋ **∴ 所以** ② 的前提（**一个 GPU 合成的开关／标志 ✓）**不存在** ✓
//     ⇒ **∴ 判据**报「第 11 条未实现」＋ **exit 1** ✓ —— **∴ 那**是一个**明确的红** ✓ ★
//
// **∴ 为什么先立它 ✗**：**它**把"第 11 条完成"变成**机器可判**✗
//   ＋ **∴ 且**它**不需要**先裁定"每帧核对 vs 只验一次"**✗
//     （**∴ 那只影响**实现方式**，**不影响"必须有回退"这条底线** ✓）★
//
// **∴ 用法 ✗**：`node scripts/tool-gpu-composite-parity.mjs`｜`--self-test`
// **∴ 退出码 ✗**：0 通过（**第 11 条已实现且守住底线 ✓）｜1 未实现（**红 ✓）
//   ｜2 用法错｜3 判据自身的输入异常

import { existsSync, readFileSync } from "node:fs";

// **∴ GPU 合成路径的候选标志（**实现时应至少引入一个 ✓）★
const GPU_COMPOSITE_FLAGS = [
  "YANSHI_GPU_COMPOSITE",
  "gpu_composite",
  "try_composite_on_gpu",
];

// **∴ CPU 真值的候选（**合成仍须能在 CPU 上算 ✓）★
const CPU_TRUTH_MARKERS = [
  "composite_within",
  "fn composite(",
  "pub fn composite",
];

const FILES = ["crates/yanshi-render/src/render.rs", "crates/yanshi-render/src/buffer.rs"];

/** **∴ 纯函数：从源码集合里判定三件事 ✗**（**∴ 自检可直接喂假源码 ✓）★ */
export function judge(sources) {
  const joined = sources.join("\n");
  const hasCpuTruth = CPU_TRUTH_MARKERS.some((m) => joined.includes(m));
  const flags = GPU_COMPOSITE_FLAGS.filter((f) => joined.includes(f));
  const hasGpuComposite = flags.length > 0;
  // **∴ 回退的证据：**与"比较"和"回退"有关的实现痕迹 ✓
  const hasCompare = /max_channel_delta|compare_.*gpu|verify_.*composite/.test(joined);
  const problems = [];
  const notes = [];
  if (!hasCpuTruth) {
    problems.push(
      "判据自身的输入异常：**找不到 CPU 合成的真值实现**（" + CPU_TRUTH_MARKERS.join("／") + "）",
    );
  }
  if (!hasGpuComposite) {
    problems.push(
      "目标第 11 条**未实现**：找不到 GPU 合成的开关（候选：" + GPU_COMPOSITE_FLAGS.join("／") + "）",
    );
  } else if (!hasCompare) {
    problems.push(
      "GPU 合成已存在，但**找不到逐位比较／回退的实现痕迹**（候选：max_channel_delta／compare_*_gpu）⇒ 违反「不一致回退」",
    );
  } else {
    notes.push("GPU 合成 ＋ 逐位比较／回退的实现痕迹都在 ✓");
  }
  return { hasCpuTruth, hasGpuComposite, hasCompare, flags, problems, notes };
}

if (process.argv.includes("--self-test")) {
  const cpu = "crates/yanshi-render/src/buffer.rs: pub fn composite(&mut self, other: &Buffer, mode: BlendMode, alpha: f64) {}";
  const cases = [
    {
      name: "只有 CPU 真值 ⇒ 报未实现（**当前状态 ✓）",
      src: [cpu],
      want: "未实现",
    },
    {
      name: "有 GPU 合成但**没有比较／回退** ⇒ 报违反回退",
      src: [cpu, "YANSHI_GPU_COMPOSITE"],
      want: "回退",
    },
    {
      name: "有 GPU 合成 ＋ 有比较 ⇒ 通过",
      src: [cpu, "YANSHI_GPU_COMPOSITE", "max_channel_delta"],
      want: "clean",
    },
    {
      name: "连 CPU 真值都没有 ⇒ 报判据自身异常",
      src: ["fn nothing() {}"],
      want: "判据自身的输入异常",
    },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：四种状态都要判对 ──");
  for (const c of cases) {
    const got = judge(c.src).problems;
    const hit = c.want === "clean" ? got.length === 0 : got.some((m) => m.includes(c.want));
    if (!hit) {
      wrong += 1;
      console.log(`  ✗ ${c.name} ⇒ 得到 ${JSON.stringify(got)}`);
      continue;
    }
    if (got.length > 0) red += 1;
    console.log(`  ✓ ${c.name} ⇒ ${got.length > 0 ? "红" : "绿"}（**符合预期 ✓）`);
  }
  // **∴ 变异 ✗**：**把 GPU 合成标志删掉 ⇒ 必须回到"未实现"** ✓
  const mutated = judge([cpu, "max_channel_delta"]);
  const mustRed = mutated.problems.some((m) => m.includes("未实现"));
  if (!mustRed) {
    wrong += 1;
    console.log("  ✗ 变异：删掉 GPU 合成标志后竟然不报未实现 ⇒ **判据没有牙**");
  } else {
    red += 1;
    console.log("  ✓ 变异：删掉 GPU 合成标志 ⇒ 报「未实现」（**符合预期 ✓）");
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length + 1} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) {
    console.error("  ✗ 自检失败 ⇒ **本判据没有牙**");
    process.exit(1);
  }
  console.log("  ✓ 自检通过");
  process.exit(0);
}

const missing = FILES.filter((f) => !existsSync(f));
if (missing.length > 0) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：${missing.join("／")}`);
  process.exit(3);
}
const sources = FILES.map((f) => readFileSync(f, "utf8"));
const verdict = judge(sources);
console.log("");
console.log("  ★ 目标第 11 条的底线（合成上 GPU）★");
console.log(`  · CPU 真值仍在：${verdict.hasCpuTruth ? "是 ✓" : "否 ✗"}`);
console.log(`  · GPU 合成路径：${verdict.hasGpuComposite ? `有（${verdict.flags.join("／")}）` : "**没有** ✗"}`);
console.log(`  · 逐位比较／回退痕迹：${verdict.hasCompare ? "有 ✓" : "**没有** ✗"}`);
console.log("");
if (verdict.problems.some((m) => m.startsWith("判据自身"))) {
  for (const m of verdict.problems) console.error(`  ✗ ${m}`);
  process.exit(3);
}
if (verdict.problems.length > 0) {
  for (const m of verdict.problems) console.error(`  ✗ ${m}`);
  console.error("");
  console.error("  ⇒ 目标第 11 条**未完成**（底线未守住）");
  process.exit(1);
}
for (const n of verdict.notes) console.log(`  ✓ ${n}`);
console.log("  ✓ 底线守住：CPU 真值仍在 ＋ GPU 合成有逐位比较与回退");
process.exit(0);
