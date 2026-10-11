#!/usr/bin/env node
// **★ macOS 上的 GPU 量化微基准 ✗ ★**（**2026-10-11 用户要求「给 macOS 也加 L4 那样的测试」✓）
//
// **∴ 为什么要单独一条 ✗**：`tool-macos-gpu-benefit.mjs` 测的是**端到端** `render_region` ✓
//   ⇒ **∴ 而** L4 上的规模曲线（`gpu_scale_sweep`）测的是**纯量化**（f32→u8，不含合成 ✓）
//     ⇒ **∴ 两种口径**不能直接比** ✓
//       ⇒ **∴ 于是**：本脚本只测量化一步 ✓，与 L4 **同口径** ✓ ★**** ✓✓
//
// **∴ 它量什么 ✗**：
//   **∴ ① 时间账 ✗**：**墙钟**（每档 5 次取中位数 ✓，与 L4 一致 ✓）
//   **∴ ② CPU 占用账 ✗**：**进程 CPU 时间**（`getrusage`，Rust binary 内测 ✓，macOS/Linux 通用 ✓）
//   **∴ ③ 逐位一致 ✗**：`max_channel_delta` 必须为 0 ✓（**∴ GPU 与 CPU 逐位一致 ✓**）
//   **∴ ④ 适配器 ✗**：打印 `adapter_note` ✓（**∴ macOS 上应含 `metal`** ✓）
//
// **∴ 规模档 ✗**（**与 L4 完全一致 ✓**）：
//   `1K／3K／10K／30K／100K／300K／1M／4M` 像素 ✓
//
// **∴ 它怎么测 ✗**：
//   **∴ ①** 先构建 helper binary：
//     `cargo build --release -p yanshi-http --bin gpu_quantize_bench --features gpu`
//   **∴ ②** 一次跑完全部 8 档（binary 内循环，避免进程启动开销 ✓）
//   **∴ ③** 输入与 L4 **相同的伪随机预乘像素** ✓（`0x1234_5678` LCG ✓）
//   **∴ ④** LUT 与 L4 **同一张表** ✓（`yanshi_render::color::srgb_encode_table` ✓）
//
// **∴ 用法 ✗**：
//   node scripts/tool-macos-gpu-quantize-bench.mjs [--binary <path>] [--sizes 1k,3k,...] [--runs 5]
//   **∴ 在非 macOS 上开发本脚本 ✗**：`YANSHI_ALLOW_NON_MACOS=1 node …` ✓
//   **∴ 自检 ✗**：`--self-test`（只验参数解析与表格渲染，不调 binary ✓）
//
// **∴ 退出码 ✗**：**0 ＝ 全部 `ok` 且逐位一致**✗｜**1 ＝ 有 `!ok` 或 delta 非 0**✗
//   ｜**2 ＝ 环境／用法问题**✗｜**3 ＝ 判据自身跑不了**（**不是产品问题 ✓**）

import { execSync, execFileSync } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";

// ─────────────────────────── 参数 ───────────────────────────
const argv = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && i + 1 < argv.length ? argv[i + 1] : fallback;
};
const has = (name) => argv.includes(name);

const binary = opt("--binary", "target/release/gpu_quantize_bench");
const sizesArg = opt("--sizes", "1k,3k,10k,30k,100k,300k,1m,4m");
const runs = Number(opt("--runs", "5"));
const outPath = opt("--out", "target/macos-gpu-quantize-bench.md");
const jsonPath = opt("--json", "target/macos-gpu-quantize-bench.json");
const selfTest = has("--self-test");

const isMac = process.platform === "darwin";
const allowNonMac = process.env.YANSHI_ALLOW_NON_MACOS === "1";

// **∴ 规模名 → 像素数 ✗**（**与 L4 的 8 档一致 ✓**）
const SIZE_MAP = {
  "1k": 1_000, "3k": 3_000, "10k": 10_000, "30k": 30_000,
  "100k": 100_000, "300k": 300_000, "1m": 1_000_000, "4m": 4_000_000,
};

function parseSizes(arg) {
  const out = [];
  for (const s of arg.split(",")) {
    const k = s.trim().toLowerCase();
    if (!SIZE_MAP[k]) {
      console.error(`  ✗ 未知规模名：${s}（可选：${Object.keys(SIZE_MAP).join("／")}）`);
      process.exit(2);
    }
    out.push(SIZE_MAP[k]);
  }
  return out;
}

// ─────────────────────────── 自检 ───────────────────────────
if (selfTest) {
  const cases = [
    ["1k,4m", [1000, 4000000]],
    ["30k", [30000]],
  ];
  let ok = true;
  for (const [input, want] of cases) {
    const got = parseSizes(input);
    const pass = JSON.stringify(got) === JSON.stringify(want);
    console.log(`  ${pass ? "✓" : "✗"} parseSizes(${input}) = ${JSON.stringify(got)}`);
    if (!pass) ok = false;
  }
  // **∴ 表格渲染自检 ✓**
  const row = renderRow({ n: 30000, cpu_ms: 3.2, gpu_ms: 0.4, cpu_used_ms: 3.0, gpu_used_ms: 0.1, max_channel_delta: 0, ok: true });
  console.log(`  ${row.includes("87.5") ? "✓" : "✗"} 表格行渲染：${row.trim().slice(0, 60)}…`);
  process.exit(ok ? 0 : 1);
}

// ─────────────────────── 判据自身的前提 ───────────────────────
if (!isMac && !allowNonMac) {
  console.error("  ✗ 本判据只在 macOS 上有意义（不是产品缺陷）。");
  console.error("     ⇒ 在 macOS 上跑：node scripts/tool-macos-gpu-quantize-bench.mjs");
  console.error("     ⇒ 在别处开发本脚本：YANSHI_ALLOW_NON_MACOS=1 node scripts/tool-macos-gpu-quantize-bench.mjs");
  process.exit(3);
}
if (!existsSync(binary)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到二进制 ${binary}`);
  console.error("     ⇒ 先构建：cargo build --release -p yanshi-http --bin gpu_quantize_bench --features gpu");
  process.exit(3);
}

const sizes = parseSizes(sizesArg);
console.log("");
console.log("  ★ macOS GPU 量化微基准（与 L4 同口径）★");
console.log(`  平台 = ${process.platform}｜二进制 = ${binary}｜规模 = ${sizesArg}｜每档 ${runs} 次取中位数`);

// ─────────────────────────── 运行 ───────────────────────────
let raw;
try {
  raw = execFileSync(binary, ["--sizes", sizes.join(","), "--runs", String(runs)], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    timeout: 600_000,
  });
} catch (e) {
  // **∴ binary 用 exit 3 表示「GPU 不可用」✗ ⇒ **∴ 那是**判据自身跑不了** ✓**
  const msg = String((e.stdout || "") + (e.stderr || ""));
  if (msg.includes('"error"')) {
    console.error(`  ✗ GPU 不可用（不是产品缺陷）：${msg.slice(0, 200)}`);
    process.exit(3);
  }
  console.error(`  ✗ binary 执行失败：${String(e.message).slice(0, 200)}`);
  process.exit(2);
}

let rows;
try {
  rows = JSON.parse(raw);
} catch (e) {
  console.error(`  ✗ binary 输出不是合法 JSON：${raw.slice(0, 200)}`);
  process.exit(2);
}
if (!Array.isArray(rows) || rows.length === 0) {
  console.error("  ✗ binary 返回空结果");
  process.exit(2);
}

// ─────────────────────────── 表格 ───────────────────────────
function pct(a, b) {
  // **∴ 正数 ＝ GPU 更快／更省 ✓**（**与 L4 表格同口径 ✓**）
  if (!isFinite(a) || !isFinite(b) || a <= 0) return "—";
  const v = ((a - b) / a) * 100;
  return (v >= 0 ? "+" : "") + v.toFixed(1) + "%";
}

function renderRow(r) {
  const timeAcct = pct(r.cpu_ms, r.gpu_ms);
  const cpuAcct = pct(r.cpu_used_ms, r.gpu_used_ms);
  const mark = !r.ok ? "✗" : (r.gpu_ms < r.cpu_ms ? "★" : "");
  return `| ${r.n.toLocaleString()} | ${r.cpu_ms.toFixed(1)} | ${r.gpu_ms.toFixed(1)} | ${mark} ${timeAcct} | ${cpuAcct} | ${r.max_channel_delta} |`;
}

const lines = [];
lines.push("## macOS GPU 量化微基准（纯 f32→u8，与 L4 同口径）");
lines.push("");
lines.push(`- 平台：\`${process.platform}\`｜二进制：\`${binary}\``);
lines.push(`- 适配器：\`${String(rows[0].adapter_note || "（空）").slice(0, 100)}\``);
lines.push(`- 每档 ${runs} 次取中位数｜输入：LCG 伪随机预乘像素（与 L4 相同种子 ✓）`);
lines.push("");
lines.push("| n（像素） | CPU ms | GPU ms | 时间账 | CPU 占用账 | max_delta |");
lines.push("|---|---|---|---|---|---|");
for (const r of rows) lines.push(renderRow(r));
lines.push("");
lines.push("**口径说明**：时间账／CPU 占用账为正数＝GPU 更快／更省；`max_channel_delta` 必须为 0（逐位一致 ✓）。");
lines.push("");

// **∴ macOS 硬约束 ✗**：适配器说明必须含 `metal`（**∴ 同 `tool-macos-gpu-benefit.mjs` 第 ③ 条 ✓）
let exitCode = 0;
const adapterNote = String(rows[0].adapter_note || "");
if (isMac && !/metal/i.test(adapterNote)) {
  lines.push(`⚠️ 适配器说明不含 metal：\`${adapterNote.slice(0, 80)}\` ⇒ 可能是软件渲染，数据仅供参考。`);
  lines.push("");
}
for (const r of rows) {
  if (!r.ok || r.max_channel_delta !== 0) {
    lines.push(`❌ n=${r.n}：ok=${r.ok}，max_channel_delta=${r.max_channel_delta}（必须为 0）`);
    exitCode = 1;
  }
}

// **∴ 找平衡点 ✗**（**与 L4 表格的「3K 像素就盈利」对应 ✓**）
let breakeven = null;
for (const r of rows) {
  if (r.ok && isFinite(r.gpu_ms) && r.gpu_ms < r.cpu_ms) { breakeven = r.n; break; }
}
if (breakeven != null) {
  lines.push(`**盈亏平衡点**：${breakeven.toLocaleString()} 像素起 GPU 更快（本机实测 ✓）。`);
} else {
  lines.push("**盈亏平衡点**：8 档内 GPU 均未快过 CPU（本机实测 ✓）。");
}
lines.push("");

const md = lines.join("\n");
console.log("");
console.log(md);
writeFileSync(outPath, md);
writeFileSync(jsonPath, JSON.stringify(rows, null, 2));
console.log(`  已写入：${outPath} ＋ ${jsonPath}`);
process.exit(exitCode);
