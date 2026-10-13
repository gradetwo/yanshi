#!/usr/bin/env node
//! **★ 两本账的报告器：**墙钟永远在第一列**，并自动标出**假收益** ✗ ★**
//!
//! **∴ 为什么造它 ✗**（**第 804 轮；**用户直接指令 ＋ 两次代价 ✓）★**：
//!   **∴ 用户在会话中指出 ✗**：**「**经常犯的错误要考虑造工具简化自己的操作和避免错误**」
//!     ＋ **∴ 而**「**只看分段、不看墙钟**」**推翻过我**两次结论**✗：
//!       ⇒ **∴ ① 第 4 项（**分块 ✓）✗**：**我**报「**256 MiB 净收益 67.6 ms**」
//!         ＋ **∴ 而**源码里**早写着** 4 MiB 才是墙钟最优（**32 MiB 差 13.2% ✓）★
//!       ⇒ **∴ ② 第 5 项（**读回池 ✓）✗**：**我**报「**省 9%／52%**」
//!         ＋ **∴ 而** 15 轮的墙钟复核显示**分段差在噪声内**（**+0.3 ms ✓）★
//!   **∴ 根因 ✗**：**分段是**定位工具**✗ 而**不是**判据** ✓
//!     ＋ **∴ 所以**：**报告器**必须让**墙钟**在视觉上**无法被忽略** ✓ ★★★
//!
//! **∴ 它做什么 ✗**（**四件事 ✓）★**：
//!   ⇒ **∴ ①** **墙钟（**min／中位／max ＋ CI ＋ MAD ✓）**放**第一列组**** ✓
//!     ⇒ **∴ ②** 分段（**upload／submit／readback／gpu ✓）**放到**后面**✗ 并**标明"仅用于定位"** ✓
//!       ⇒ **∴ ③** **自动判定假收益**✗：**若**分段合计改善 ≥10%**✗ 而**墙钟改善 <2%**（**或**反而更差 ✓）
//!         ⇒ **∴ 就**打印 `⚠ 疑似假收益`**✗ 并**列出两边的数** ✓ ★★★
//!           ⇒ **∴ ④** **多份日志**可以并排比（**∴ 从而**一眼看出哪个配置真的快 ✓）★
//!
//! **∴ 用法 ✗**：
//!   node scripts/report-accounts.mjs /tmp/a.log /tmp/b.log …
//!   node scripts/report-accounts.mjs --label A=pool1.log --label B=pool0.log
//!
//! **∴ 退出码 ✗**：0 报告已出（**∴ 不判快慢 ✓）｜**1** 读不到任何日志（**前置不满足 ✓）｜2 用法错
//!
//! **∴ 口径（**用户已裁定 ✓）★**：
//!   ＋ **∴ 时间账**只在独显上报**✗ ⇒ **∴ 核显只报 CPU 占用** ✓
//!     ＋ **∴ 所以**：**本报告器**对 `disabled_by_flag` 那一行**只打 CPU 占用**✗ 而**不比墙钟** ✓

import { readFileSync, existsSync } from "node:fs";
import { basename } from "node:path";

const argv = process.argv.slice(2);
const files = [];
const labels = [];
for (let i = 0; i < argv.length; i += 1) {
  const a = argv[i];
  if (a === "--label") {
    const spec = argv[i + 1] || "";
    const eq = spec.indexOf("=");
    if (eq < 0) {
      console.error(`  ✗ --label 的格式应当是 名字=路径（**收到 ${spec} ✓）`);
      process.exit(2);
    }
    labels.push(spec.slice(0, eq));
    files.push(spec.slice(eq + 1));
    i += 1;
    continue;
  }
  if (a.startsWith("--")) {
    console.error(`  ✗ 未知参数：${a}`);
    process.exit(2);
  }
  labels.push(basename(a));
  files.push(a);
}
if (files.length === 0) {
  console.error("  用法：node scripts/report-accounts.mjs [--label 名=路径] <日志…>");
  process.exit(2);
}

/** **∴ 从一份日志里抽出**一个配置的**账**** ✓（**∴ 与** `tool-render-cost-accounts.mjs` 的输出格式对齐 ✓）*/
function parseOne(text) {
  const out = { adapter: null, wall: null, cpu: null, mad: null, ci: null, stages: null, delta: null, blobs: null, rounds: null };
  // **∴ 适配器（**必须打印 ⇒ **∴ 报告器**自己再强调一次 ✓）★
  const ad = /适配器\s*＝\s*(adapter:[^\s]+)/.exec(text);
  if (ad) out.adapter = ad[1];
  const disabled = /适配器\s*＝\s*disabled_by_flag/.test(text);
  // **∴ 账行 ✗**：**`| GPU | gpu（--gpu on） | 1295／1452／1665 | [a–b] | 129 | … |`
  const row = /\|\s*GPU\s*\|\s*gpu[^|]*\|\s*([\d.]+)[／/]([\d.]+)[／/]([\d.]+)\s*\|\s*\[?([^|\]]*)\]?\s*\|\s*([\d.]+)\s*\|/.exec(text);
  if (row) {
    out.wall = { min: Number(row[1]), median: Number(row[2]), max: Number(row[3]), ci: row[4].trim(), mad: Number(row[5]) };
  }
  const cpuRow = /\|\s*GPU\s*\|[^|]*\|\s*[\d.]+[／/][\d.]+[／/][\d.]+\s*\|[^|]*\|\s*[\d.]+\s*\|\s*([\d.]+)[／/]([\d.]+)[／/]([\d.]+)/.exec(text);
  if (cpuRow) out.cpu = { min: Number(cpuRow[1]), median: Number(cpuRow[2]), max: Number(cpuRow[3]) };
  // **∴ 分段 ✗**：**`GPU：calls ＝ 20｜upload ＝ 33435｜submit ＝ 315｜readback ＝ 81697｜gpu ＝ 750`** ✓
  const st = /GPU：calls\s*＝\s*(\d+)｜upload\s*＝\s*(\d+)｜submit\s*＝\s*(\d+)｜readback\s*＝\s*(\d+)｜(?:★)?gpu(?:★)?\s*＝\s*(\d+)/.exec(text);
  if (st) {
    out.stages = { calls: Number(st[1]), upload: Number(st[2]), submit: Number(st[3]), readback: Number(st[4]), gpu: Number(st[5]) };
  }
  const d = /max_channel_delta\s*＝\s*\[([^\]]*)\]/.exec(text);
  if (d) out.delta = d[1].trim();
  const b = /verified_blobs\s*＝\s*\[([^\]]*)\]/.exec(text);
  if (b) out.blobs = b[1].trim();
  const rr = /有效轮数[^|]*\|\s*(\d+)\s*\|/.exec(text);
  if (rr) out.rounds = Number(rr[1]);
  out.disabled = disabled && !out.adapter;
  // **∴ 核显口径 ✗**：**只报 CPU 占用**✗ 而**不比墙钟** ✓
  const cpuOnlyRow = /\|\s*CPU\s*\|\s*cpu[^|]*\|\s*[\d.]+[／/]([\d.]+)[／/][\d.]+\s*\|[^|]*\|\s*[\d.]+\s*\|\s*([\d.]+)[／/]([\d.]+)[／/]([\d.]+)/.exec(text);
  if (cpuOnlyRow) out.cpuOnly = { median: Number(cpuOnlyRow[2]) };
  return out;
}

const reports = [];
for (let i = 0; i < files.length; i += 1) {
  const f = files[i];
  if (!existsSync(f)) {
    console.error(`  ✗ 读不到日志（**前置不满足，不是判据失败 ✓）：${f}`);
    continue;
  }
  reports.push({ label: labels[i], path: f, ...parseOne(readFileSync(f, "utf8")) });
}
if (reports.length === 0) {
  console.error("  ✗ 没有任何可读的日志 ⇒ **∴ 前置不满足** ✓");
  process.exit(1);
}

// **∴ 分段合计**必须**现算** ✗ ★**（**第 805 轮；**我自己踩的 ✓）
//   ＋ **∴ 我**第一版把它存成 `out.stagesTotal`**✗
//     ⇒ **∴ 而** `{ ...parseOne() }` 的展开会**带上它**✗ 可**打印却出 undefined**
//       ⇒ **∴ 于是**假收益检测**拿到 NaN**✗ ⇒ **∴ 它**在该报警时说了"一致"** ✓ ★★★
//         ＋ **∴ 所以**：**一律**现算**✗ **不存派生字段** ✓
const stagesTotal = (r) => (r.stages ? r.stages.upload + r.stages.submit + r.stages.readback : null);
const fmt = (v, d = 0) => (v === null || v === undefined || Number.isNaN(v) ? "—" : Number(v).toFixed(d));

console.log("");
console.log("  ★ 两本账（**墙钟永远第一列组**✗ 分段**只用于定位** ✓）★");
console.log("     **∴ 口径 ✗**：**时间账**只在独显上报**✗（**核显那一行**只报 CPU 占用** ✓）★");
console.log("");
console.log("  | 配置 | 适配器 | ① 墙钟 min／中位／max (ms) | 中位 95% CI | **MAD** | ② CPU 中位 (ms) | 有效轮 | 逐位一致 |");
console.log("  |---|---|---|---|---|---|---|---|");
for (const r of reports) {
  const w = r.wall;
  const ad = r.adapter ? r.adapter.replace("adapter:Vulkan:", "").replace(":device=true", "") : (r.disabled ? "（**核显／无 GPU ⇒ 只报 CPU ✓）" : "—");
  console.log(`  | ${r.label} | ${ad} | ${w ? `${fmt(w.min)}／**${fmt(w.median)}**／${fmt(w.max)}` : "—"} | ${w && w.ci ? w.ci : "—"} | ${w ? fmt(w.mad) : "—"} | ${r.cpu ? fmt(r.cpu.median) : "—"} | ${r.rounds ?? "—"} | ${r.delta ?? "—"} ← blobs ${r.blobs ?? "—"} |`);
}
console.log("");
console.log("  ★ 分段（**微秒 ✗** —— **∴ 只用于**定位瓶颈**✗ 不用于**判收益** ✓）★");
console.log("");
console.log("  | 配置 | calls | upload | submit | readback | gpu | 分段合计 |");
console.log("  |---|---|---|---|---|---|---|");
for (const r of reports) {
  const s = r.stages;
  console.log(`  | ${r.label} | ${s ? s.calls : "—"} | ${s ? s.upload : "—"} | ${s ? s.submit : "—"} | ${s ? s.readback : "—"} | ${s ? (s.gpu || "0（**探针未开 ✓）") : "—"} | ${s ? stagesTotal(r) : "—"} |`);
}

// **★★★ 假收益检测 ✗ ★★★**（**∴ 本工具的核心价值 ✓）
console.log("");
console.log("  ★ 假收益检测（**分段变好 ≠ 墙钟变好** ✓）★");
console.log("");
let warned = 0;
const withBoth = reports.filter((r) => r.wall && stagesTotal(r) !== null);
for (let i = 1; i < withBoth.length; i += 1) {
  const a = withBoth[0];
  const b = withBoth[i];
  const dStage = (stagesTotal(b) - stagesTotal(a)) / stagesTotal(a);
  const dWall = (b.wall.median - a.wall.median) / a.wall.median;
  const flag = dStage <= -0.1 && dWall > -0.02;
  const verdict = flag ? "⚠ **疑似假收益**（**分段改善 ≥10% ✗ 而**墙钟几乎没动** ✓）" : "（**一致 ✓）";
  console.log(`  · ${a.label} ⇒ ${b.label}：分段合计 ${(dStage * 100).toFixed(1)}%｜**墙钟中位 ${(dWall * 100).toFixed(1)}%** ${verdict}`);
  if (flag) {
    warned += 1;
    console.log(`      ⇒ **∴ 所以**：**不要**声称 ${b.label} 更快**✗ —— **∴ 分段**是被别处的开销吃掉的** ✓`);
    console.log(`      ⇒ **∴ 而**墙钟的 MAD：${a.label} ${fmt(a.wall.mad)}｜${b.label} ${fmt(b.wall.mad)}（**∴ 若**差值小于 MAD ⇒ **∴ 那**就是噪声** ✓）`);
  }
}
if (withBoth.length < 2) {
  // **★★★ 「无法判定」必须是一个**显式结论** ✗ ★★★**（**第 805 轮 ✓）
  //   ＋ **∴ 为什么 ✗**：**只有一份日志（**或**缺分段 ✓）⇒ **∴ 我**什么也**不能判**
  //     ＋ **∴ 而**说"一致／没有假收益"**✗ ⇒ **∴ 那**是**把"没测"说成"测了"** ✓ ★★
  const missing = reports.filter((r) => r.wall === null).map((r) => r.label);
  const noStages = reports.filter((r) => stagesTotal(r) === null).map((r) => r.label);
  console.log(`  · **∴ 无法判定 ✗**：带墙钟 **＋** 分段的日志只有 ${withBoth.length} 份（**需要 ≥2 ✓）`);
  if (missing.length) console.log(`      ⇒ 缺**墙钟**：${missing.join("／")}`);
  if (noStages.length) console.log(`      ⇒ 缺**分段**：${noStages.join("／")}`);
  console.log("      ⇒ **∴ 所以**：**本报告**不给出「有没有假收益」的结论** ✓");
} else if (warned === 0) {
  console.log(`  · ✓ 在这 ${withBoth.length} 份日志之间没有发现假收益（**分段与墙钟**方向一致** ✓）`);
}

console.log("");
console.log("  ⇒ **∴ 判据的顺序 ✗**：**① 先看**墙钟**✗ ② 再看** MAD 是否容许差异**✗ ③ **最后**才用分段**定位** ✓");
process.exit(0);
