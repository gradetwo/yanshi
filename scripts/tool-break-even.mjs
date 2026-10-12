//! **★ 平衡点扫描：**GPU 与 CPU 的**交点**在哪个像素数** ✗ ★**（**第 758 轮 ✓；**目标第 3 条 ✓）
//!
//! **∴ 为什么单独写一个 ✗**（**而不是改 `tool-render-cost-accounts.mjs` ✓）★**：
//!   **∴ 那个工具**已经把单点的账做全了**✗**：
//!     ＋ **∴ ① 两本账（**墙钟 ＋ CPU ✓）＋ **min／中位／max ＋ 95% CI ＋ MAD** ✓
//!       ＋ **∴ ② **适配器全部打印**✗（**第 581 轮：**只打第一个会误导 ✓）★
//!         ＋ **∴ ③ **GPU 阶段分段**✗（**传输 vs 计算 ✓）＋ **④ **哨兵核对结果** ✓
//!           ＋ **∴ 而**它**唯一的缺口**✗：**它**在一个尺寸上跑**✗ 而**不**扫一组尺寸** ✓
//!             ＋ **∴ 所以**：**这里**只做**循环 ＋ 解析 ＋ 找交点**✗（**∴ 不重复实现测量 ✓）★
//!
//! **∴ 为什么必须打印**适配器名 ✗**（**第 318 轮的坑 ✓）★**：
//!   **∴ 不打印**就会把 `llvmpipe`（**软件光栅器 ✓）当成硬件**✗ ⇒ **∴ 于是**所有结论都错** ✓
//!     ＋ **∴ 而**本工具**把适配器**透传打印**✗（**∴ 从子工具的输出里抓 ✓）★
//!
//! **∴ 退出码（**与全仓库判据同一约定 ✓）✗**：
//!   **∴ 0 ✗**：**找到交点**（**∴ 并打印它 ✓）
//!     ＋ **∴ 1 ✗**：**扫描跑完但**没找到交点**✗（**∴ 如**全区间 GPU 都更快或都更慢 ✓）
//!       ＋ **∴ 2 ✗**：**用法／前置条件错**✗（**如**子工具不存在 ✓）
//!         ＋ **∴ 3 ✗**：**判据自身的前置条件缺失**✗ ⇒ **∴ 本机**没有 GPU 适配器** ✓ ★
import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";

const argv = process.argv.slice(2);
const option = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};

const TOOL = "scripts/tool-render-cost-accounts.mjs";
if (!existsSync(TOOL)) {
  console.error(`  ✗ 找不到 ${TOOL} ⇒ 前置条件错（exit 2）`);
  process.exit(2);
}
// **∴ 默认扫的像素数 ✗**：**从 8k 到 4M**✗ ⇒ **∴ 覆盖**核显 1M**与**可能的独显平衡点** ✓ ★
const defaultSides = "90,128,181,256,362,512,724,1024,1448,2048";
const sides = option("--sides", defaultSides).split(",").map((n) => Number(n.trim())).filter(Boolean);
const rounds = option("--rounds", "5");
const extra = argv.includes("--verify") ? ["--verify"] : [];

console.log(`  扫描 ${sides.length} 个尺寸｜每尺寸 ${rounds} 轮｜适配器名必须打印（第 318 轮）`);
console.log("");

const rows = [];
let adapterSeen = null;
let adapterNote = null;
for (const side of sides) {
  const px = side * side;
  let out = "";
  try {
    out = execFileSync(
      process.execPath,
      [TOOL, "--region", `${side}x${side}`, "--rounds", rounds, ...extra],
      { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
    );
  } catch (error) {
    out = String((error && error.stdout) || "") + String((error && error.stderr) || "");
  }
  // **∴ 抓适配器名 ✓（**∴ 只抓一次 ✗ 而**不**假设每个尺寸都报 ✓）★
  // **★★★ 必须区分「**真适配器名**」与「**渲染节点存在**」 ✗ ★★★**（**第 758 轮 ✓）
  //   **∴ 为什么 ✗**（**第 318 轮那个坑的同形物 ✓）★**：
  //     **∴ 子工具在**无 GPU 时**会报 `gpu_adapter_note ＝ render_node_present`** ✗
  //       ＋ **∴ 而**那**看起来像**适配器名**✗ ⇒ **∴ 若**直接打印它** ✓
  //         ＋ **∴ 读的人**会以为**有硬件**✗ ⇒ **∴ 于是**所有结论都错** ✓ ★★
  //           ＋ **∴ 所以 ✗**：**它**必须被识别为**「**没有真名**」** ✗ 而**不是**一个名字** ✓ ★
  const NOT_A_NAME = new Set(["render_node_present", "none", "null", "unknown", "software"]);
  for (const m of out.matchAll(/适配器\s*＝\s*(.+)/g)) {
    const value = m[1].trim();
    if (NOT_A_NAME.has(value)) {
      adapterNote = value; // **∴ 记住它 ⇒ **∴ 用于说明「**为什么没有名字**」 ✓
      continue;
    }
    if (!adapterSeen) adapterSeen = value;
  }
  // **∴ 抓「后端」那一行的两本账 ✓
  //   **∴ 行形如 ✗**：**`| 配置 | gpu | ① 12.3（min／中位／max） | … | ② 45.6（…） |`
  //     ＋ **∴ 而**本机**无 GPU 时**只有 cpu 行**✓ ⇒ **∴ 那**正是 exit 3 的情形** ✓ ★
  const gpu = /gpu[^|]*\|\s*([\d.]+)\s*（/.exec(out);
  const cpu = /cpu[^|]*\|\s*([\d.]+)\s*（/.exec(out);
  const wall = /中位[^|]*?([\d.]+)/.exec(out);
  const row = {
    side,
    px,
    gpuMs: gpu ? Number(gpu[1]) : null,
    cpuMs: cpu ? Number(cpu[1]) : null,
    wall: wall ? Number(wall[1]) : null,
  };
  rows.push(row);
  const shape = row.gpuMs !== null && row.cpuMs !== null
    ? `GPU ${row.gpuMs.toFixed(2)} ms｜CPU ${row.cpuMs.toFixed(2)} ms｜比 ${(row.gpuMs / row.cpuMs).toFixed(2)}×`
    : "（本尺寸没拿到两路 ⇒ 见下方判读）";
  console.log(`  ${String(side).padStart(4)}² ＝ ${String(px).padStart(9)} px｜${shape}`);
}

console.log("");
// **∴ 适配器的**三种情形**要分开说 ✗**（**∴ 不许**把「没有名字」当成名字 ✓）★
if (adapterSeen) {
  console.log(`  适配器 ＝ ${adapterSeen}            ← ★ 真适配器名（必须打印，第 318 轮）★`);
} else if (adapterNote) {
  console.log(`  适配器 ＝ **没有拿到真名字**（子工具报的是 ${adapterNote} ⇒ 那是**渲染节点存在**，不是适配器）`);
  console.log("    ⇒ 本机的结论**不可判**（不许把软件光栅器当成硬件，第 318 轮）");
} else {
  console.log("  适配器 ＝ （子工具没报任何适配器信息）");
}
// **∴ 判定 ✗**：**需要**至少一对 (GPU, CPU) 才能谈交点** ✓ ★
const paired = rows.filter((r) => r.gpuMs !== null && r.cpuMs !== null);
if (paired.length < 2) {
  console.error("");
  console.error("  ★ 没有拿到两路账 ⇒ **本机没有可用的 GPU 适配器**（或后端没走 GPU）");
  console.error("    ⇒ 这不是失败，是**判据自身的前置条件缺失** ⇒ exit 3");
  console.error("    ⇒ 在 L4 上重跑（脚本已就绪）");
  process.exit(3);
}
// **∴ 找交点 ✗**：**GPU 首次**不慢于** CPU 的最小像素数** ✓ ★
const cross = paired.find((r) => r.gpuMs <= r.cpuMs);
if (cross) {
  console.log(`  ✓ 交点 ⇒ **${cross.px} px（${cross.side}²）**｜GPU ${cross.gpuMs.toFixed(2)} ms ≤ CPU ${cross.cpuMs.toFixed(2)} ms`);
  console.log("    ⇒ 与 `yanshi-gpu/src/lib.rs::threshold_for` 的现行值对照后决定是否重定");
  process.exit(0);
}
console.log("  ✗ 扫完整个区间都没找到交点（GPU 一直更慢，或一直更快）⇒ exit 1");
console.log("    ⇒ 扩大 --sides 区间后重跑");
process.exit(1);
