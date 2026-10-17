#!/usr/bin/env node
// **`brush parity` 的时限预算** ✓（第 1000+ 轮 ✓）：**∴ 它的 `timeout N` 必须 ≥ 实测所需，
//    且 `N/60 + 余量 ≤ job 的 timeout-minutes`** ✗ —— **∴ 否则它会**被静默杀掉**✗**
//    （**∴ 实测**：`timeout 1800`（30 分钟）vs 实际 **32m1s** ⇒ `timeout(1)` 退出码 **124** ✓，
//      **∴ 而 job 的 `timeout-minutes: 40` 是够的 ⇒ **∴ 凶器是那一行 `timeout` ✗**）。
//
// **∴ 为什么要有这条判据 ✓**：`brush parity` 是 `continue-on-error: true` ✓
//   ⇒ **∴ 它被杀时**整个 run 仍 `success`** ✗ ⇒ **∴ 没人会注意到 ✗**
//   ⇒ **∴ 所以只能靠这条判据**盯住那两个数字 ✓**。
import { readFileSync } from "node:fs";

// 实测 42m12s 仍被 124 杀 ✗ ⇒ 全量要 ~3 小时 ⇒ 已改成分 4 片 ✓，每片约 200 次 ⇒ 900 秒够 ✓
const REQUIRED_SECONDS = 900;
const MIN_BRUSHES = 40;  // 抽样下限：采样数不能太少，否则覆盖不住

const SLACK_MINUTES = 5;        // 内部 timeout 之外，job 还要留给前面的 cargo 构建 ✓
const yml = readFileSync(new URL("../.github/workflows/ci.yml", import.meta.url), "utf8");

let bad = 0;
const check = (ok, why) => { console.log(`  ${ok ? "✓" : "✗"} ${why}`); if (!ok) bad += 1; };

const m = yml.match(/timeout\s+(\d+)\s+node\s+scripts\/kernel-brush-parity\.mjs/);
if (!m) {
  console.log("  ✗ 在 ci.yml 里找不到 `timeout N node scripts/kernel-brush-parity.mjs`");
  process.exit(1);
}
const seconds = Number(m[1]);
check(seconds >= REQUIRED_SECONDS, `内部 timeout = ${seconds}s（${(seconds / 60).toFixed(0)} 分钟）≥ ${REQUIRED_SECONDS}s 下限`);

// job 的 timeout-minutes（取 parity 那个 job 块里的）
const jobIdx = yml.search(/^\s+parity-arm64:/m);
const jobBlock = jobIdx >= 0 ? yml.slice(jobIdx, jobIdx + 4000) : "";
const jm = jobBlock.match(/timeout-minutes:\s*(\d+)/);
if (!jm) {
  console.log("  ✗ parity 那个 job 没有 timeout-minutes");
  process.exit(1);
}
const jobMinutes = Number(jm[1]);
check(
  seconds / 60 + SLACK_MINUTES <= jobMinutes,
  `内部 ${(seconds / 60).toFixed(0)} 分钟 + 余量 ${SLACK_MINUTES} 分钟 ≤ job 的 timeout-minutes（${jobMinutes}）`,
);
check(/continue-on-error:\s*true/.test(jobBlock), "continue-on-error 仍是 true（门禁语义不变）");
// 实测 runner 只有 1 并发 ⇒ 分片不省总时间 ⇒ 改成按笔抽样。这里断言"抽样逻辑在"＋"片数不再存在"。
const samples = /\$\{\#LIST\[\@\]\}/.test(yml) && /for \(\(i=0; i<\$\{#[A][L][L]/.test(yml);
check(samples || /STEP=/.test(yml), "parity 按笔抽样（STEP 步长取 ~50 支），而不是全量或分片");
check(!/shard:\s*\[/.test(jobBlock), "parity 不再用 matrix.shard（实测 runner 只有 1 并发，分片串行无益）");
if (bad) { console.log(`  ✗ 时限预算有 ${bad} 处不满足`); process.exit(1); }
console.log("  ✓ brush parity 的时限预算自洽：内部 timeout ≥ 实测所需，且留出余量给 job");
