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
// **∴ 软件/虚拟适配器 ⇒ 本判据在这台机器上**不适用**✗ 退 3** ✓（**第 248 轮 ✓）★
let notEligibleAdapter = null;
for (const side of sides) {
  const px = side * side;
  let out = "";
  try {
    out = execFileSync(
      process.execPath,
      // **★★★ 必须传**两路 `--compare` ✗ ★★★**（**第 791 轮修；**L4 实测 ✓）
      //   **∴ 症状 ✗**（**L4 实测 ✓）★**：**10 个尺寸**全部报**
      //     `（本尺寸没拿到两路 ⇒ 见下方判读）`**✗
      //       ＋ **∴ 而**同一份输出里**适配器名**却**打印正确**✗**：
      //         `适配器 ＝ adapter:Vulkan:DiscreteGpu:**NVIDIA L4**:device=true`** ✓ ★★
      //   **∴ 根因 ✗**：**这一行**只传了 `--region` 与 `--rounds`**✗
      //     ＋ **∴ 而**子工具（**`tool-render-cost-accounts.mjs` ✓）**的缺省是**单路**✗
      //       ⇒ **∴ 于是**它**只跑一路 ⇒ **∴ 解析器**找不到两路 ⇒ **∴ 报"没拿到两路"** ✓ ★
      //         ＋ **∴ 而**那句报错**还把原因写成"本机没有可用的 GPU 适配器"**✗
      //           ⇒ **∴ 于是**它**与**同一份输出里的适配器名**自相矛盾** ✓ ★★★
      //   **∴ 修法 ✗**：**与 `scripts/l4.sh accounts` 用**同一对 `--compare`** ✓ ★
      //     ＋ **∴ 证据 ✗**：**同一台 L4 上**那条命令**能拿到两路**（**CPU 1529 ms ／ GPU 1452 ms ✓）** ✓ ★
      [TOOL, "--region", `${side}x${side}`, "--rounds", rounds,
        "--compare", "CPU=--gpu off", "--compare", "GPU=--gpu on", ...extra],
      { encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 600_000 },
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
  const NOT_A_NAME = new Set(["render_node_present", "none", "null", "unknown", "software",
    // **∴ 本仓 CPU 路的字段值 ✗**（**第 792 轮 L4 实测 ✓）：**它**不是适配器名** ✓
    "disabled_by_flag"]);
  for (const m of out.matchAll(/适配器\s*＝\s*(.+)/g)) {
    const value = m[1].trim();
    if (NOT_A_NAME.has(value)) {
      adapterNote = value; // **∴ 记住它 ⇒ **∴ 用于说明「**为什么没有名字**」 ✓
      continue;
    }
    if (!adapterSeen) adapterSeen = value;
    // **★★★ 第 248 轮：**软件/虚拟适配器**不许当门禁**✗ ★★★**（**CI 同树两判换来的 ✓）
    //   ＋ **∴ 症状（**可复核 ✓）✗**：**同一棵树（**`2e710046` ✓）✗
    //     **∴ run 38404263957 ⇒ `✓ tool-break-even.mjs`**✗
    //       ＋ **∴ run 38405967723 ⇒ `✗ … (EXIT=1)`｜"扫完整个区间都没找到显著更快的尺寸"** ✓
    //         ⇒ **∴ 同一棵树两种结论 ⇒ **∴ 它**是**机器相关**的 ✗ 而**不是产品回归** ✓ ★★
    //   ＋ **∴ 为什么该退 3 ✗**：**"这台机器上 GPU 一直更慢"**是**机器事实**✗
    //     ＋ **∴ 而**它**与"GPU 该不该更快"**（**产品问题 ✓）**不是一回事** ✓
    //       ＋ **∴ 仓里的约定**就是**判不了 ⇒ 退 3**（**本文件头部第 4 条 ✓）★★
    //   ＋ **∴ 判据 ✗**：**适配器串形如 `adapter:Vulkan:DiscreteGpu:NVIDIA L4:device=true`** ✓
    //     ⇒ **∴ 第三段是**设备类别**✗ ⇒ **∴ `Cpu`／`Virtual`（**软件光栅器／虚拟 ✓）
    //       一律**不适用** ✓ ★
    const parts = value.split(":");
    const klass = parts.length >= 3 ? parts[2] : "";
    // **★ `host_has_no_render_node` 也必须挡掉（2026-10-15 ✓ 实测逮到 ✓）★**
    //   **证据** ✗：CI（run `38768555926` 的 shard 2 ✓）打印的是
    //     `适配器 ＝ host_has_no_render_node` ✓ —— **这台机器没有 render node（没有 GPU）** ✓
    //   而 `NOT_A_NAME` 里有 `render_node_present`／`none`／`software` ✓ **却没有这一条** ✗
    //   ⇒ 它被当成"真适配器名" ✓ ⇒ 扫描跑完 ⇒ **exit 1** ✗（把**机器事实**当成产品红 ✗）。
    //   **所以** ✓：凡"没有渲染节点"这一族名字，一律**不适用** ⇒ 退 3 ✓（与 `Cpu`／`Virtual` 同类 ✓）。
    if (klass === "Cpu" || klass === "Virtual" || /no_render_node|no_gpu|host_has_no/.test(value)) {
      notEligibleAdapter = value;
    }
  }
  // **★★★ 按**实际表格式**抓两本账 ✗ ★★★**（**第 792 轮修；**L4 实测 ✓）
  //   **∴ 症状 ✗**（**L4 实测 ✓）★**：**10 个尺寸**全部报**「**本尺寸没拿到两路**」**
  //     ＋ **∴ 而**同一份输出里**两路**明明都在**✗（**∴ 见**下一行注释 ✓）★
  //   **∴ 根因 ✗**：**正则**按旧表格式写**✗**：
  //     `gpu[^|]*\|\s*([\d.]+)\s*（`** —— **∴ 它**要求**「gpu…|」之后**紧跟数字**
  //       ＋ **∴ 而**实际表**多了**「后端」列**✗ ⇒ **∴ 那一格是** `gpu（--gpu on）`** ✓
  //         ⇒ **∴ 于是**「gpu（--gpu on）」之后是 ` | `**✗ 而**不是数字** ⇒ **∴ 正则**永不匹配** ✓ ★★
  //   **∴ 实际行形如 ✗**（**L4 实测原文 ✓）**：
  //     `| GPU | gpu（--gpu on） | 1295／1452／1665 | [1295–1665] | 129 | 1610／1780／2000 | 1.23× | 3334 | 1418.9 | 4 |`
  //   **∴ 修法 ✗**：**匹配 `| 配置 | 后端 |` 之后**第一格数字组**（**min／中位／max ✓）★
  //     ＋ **∴ 并**同时兼容**旧表的 `① 12.3（…）` 形（**∴ 不**丢掉兼容 ✓）★
  const arm = (label) => {
    // ① 新表：`| GPU | gpu（…） | 1295／1452／1665 |`
    const m2 = new RegExp(`\\|\\s*${label}\\s*\\|[^|]*\\|\\s*([\\d.]+)\\s*[／/]`, "i").exec(out);
    if (m2) return Number(m2[1]);
    // ② 旧表：`| gpu | ① 12.3（min／中位／max） |`
    const m1 = new RegExp(`${label}[^|]*\\|\\s*(?:①\\s*)?([\\d.]+)\\s*（`, "i").exec(out);
    return m1 ? Number(m1[1]) : null;
  };
  const gpuMs = arm("GPU");
  const cpuMs = arm("CPU");
  const gpu = gpuMs !== null, cpu = cpuMs !== null;
  const wall = /中位[^|]*?([\d.]+)/.exec(out);
  const row = {
    side,
    px,
    gpuMs, cpuMs,
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
if (notEligibleAdapter) {
  // **∴ 先说清"为什么这台机器不适用" ✗ 再退 3 ✗ 不许报红** ✓
  console.log(`  适配器 ＝ ${notEligibleAdapter}   ← ★ 软件/虚拟适配器（设备类别是 Cpu/Virtual）★`);
  console.error(
    "  ★ 本判据在这台机器上**不适用**：适配器是**软件光栅器／虚拟设备** ⇒ " +
      "「GPU 与 CPU 的交点」这个问题本身不成立 ⇒ **∴ 这不是产品失败** ✗ **∴ 退出码 3** ✓",
  );
  process.exit(3);
}
if (adapterSeen) {
  console.log(`  适配器 ＝ ${adapterSeen}            ← ★ 真适配器名（必须打印，第 318 轮）★`);
} else if (adapterNote) {
  console.log(`  适配器 ＝ **没有拿到真名字**（子工具报的是 ${adapterNote} ⇒ 那是**渲染节点存在**，不是适配器）`);
  // **★★ 没有真适配器 ⇒ 立刻退 3（2026-10-15 修 ✓）：这道闸必须排在 `exit(0)` 之前 ✗✗ ★★**
  //   **为什么** ✗（本机实测 ✓）：原先"没有真名字/软件光栅器 ⇒ 3"的闸在文件后段（约 `:257` ✓），
  //     **排在**"找到交点 ⇒ `exit(0)`"（约 `:246` ✓）**之后** ✗
  //     ⇒ ⇒ 无 GPU 的机器上，扫描里只要**偶然**出现一次"显著更快" ✓ 就**先**走了 0 ✗
  //     ⇒ **假绿** ✗✗（本机连跑三次得到 **3／0／3** ✓ —— 那一次 0 就是它 ✓）。
  //   **纪律** ✓：这种机器上"扫描结果"**没有意义** ✗ ⇒ **不看结果，直接退 3** ✓。
  if (!adapterSeen || /llvmpipe|softpipe|swrast|software|zink on llvmpipe/i.test(adapterSeen)) {
    console.log("    ⇒ 适配器不适用（没有真名字／软件光栅器）⇒ **这条判据在这台机器上判不了** ✓");
    console.log("    ⇒ ⇒ 按纪律**退出 3**（判据跑不了，不是产品红）✓ —— 并且**不看扫描结果** ✗（它没有意义 ✓）");
    process.exit(3);
  }
  console.log("    ⇒ 本机的结论**不可判**（不许把软件光栅器当成硬件，第 318 轮）");
} else {
  console.log("  适配器 ＝ （子工具没报任何适配器信息）");
}
// **∴ 判定 ✗**：**需要**至少一对 (GPU, CPU) 才能谈交点** ✓ ★
const paired = rows.filter((r) => r.gpuMs !== null && r.cpuMs !== null);
if (paired.length < 2) {
  // **★★★ 原因**必须按证据分开说 ✗ ★★★**（**第 791 轮修；**L4 实测 ✓）
  //   **∴ 症状 ✗**：**旧文案**无条件**说**
  //     `★ 没有拿到两路账 ⇒ **本机没有可用的 GPU 适配器**（或后端没走 GPU）`
  //       ＋ **∴ 而**同一份输出里**刚刚**打印了**
  //         `适配器 ＝ adapter:Vulkan:DiscreteGpu:**NVIDIA L4**:device=true`** ✓ ★★
  //           ⇒ **∴ 于是**读的人**看到**两句矛盾的话**✗ 而**真因**在别处** ✓ ★
  //   **∴ 修法 ✗**：**按**手上已有的证据**分三种情形说**✗：
  //     ⇒ **∴ ①** 有真适配器名 ⇒ **∴ 那**不是"没有 GPU"**✗ 而**是**两路账没拿到** ✓
  //       ⇒ **∴ ②** 没有真名字 ⇒ **∴ 那**才是"不可判"（**第 318 轮的坑 ✓）★
  //         ⇒ **∴ ③** 一行都没有 ⇒ **∴ 子工具**根本没跑** ✓ ★
  console.error("");
  if (adapterSeen) {
    console.error(`  ★ 没有拿到两路账 —— 而**适配器是有的**（${adapterSeen}）`);
    console.error("    ⇒ 所以**不是**「本机没有 GPU」 ✗ 而是**这次调用没跑成两路** ✓");
    console.error("    ⇒ 检查：子工具是否收到**两个** `--compare`（CPU=--gpu off ／ GPU=--gpu on）");
  } else {
    console.error("  ★ 没有拿到两路账 ⇒ **本机没有可用的 GPU 适配器**（或后端没走 GPU）");
  }
  console.error("    ⇒ 这不是失败，是**判据自身的前置条件缺失** ⇒ exit 3");
  process.exit(3);
}
// **∴ 找交点 ✗**：**GPU 首次**不慢于** CPU 的最小像素数** ✓ ★
// **★★★ 交点**不接受「相等」✗ 且要求**持续**更快 ✗ ★★★**（**第 793 轮修；**L4 实测 ✓）
//   **∴ 症状 ✗**（**L4 实测 ✓）★**：**15 轮全表下**工具报**
//     `✓ 交点 ⇒ 8100 px（90²）｜GPU 9.00 ms ≤ CPU 9.00 ms`
//       ＋ **∴ 而**真实数据是：**≤724² 两路**等价**（**0.94–1.07× ✓）✗
//         ＋ **∴ 只有** ≥1024² 才**稳定更快**（**0.80／0.82／0.82× ✓）★
//   **∴ 根因 ✗**：**旧定义 ＝「**首次 `gpuMs <= cpuMs`**」✗
//     ＋ **∴ 而** `9.00 <= 9.00` **为真**✗ ⇒ **∴ 于是**第一个尺寸就成了"交点"** ✓ ★★
//   **∴ 修法 ✗**：**要求**严格更快**（**`gpuMs < cpuMs` ✓）✗
//     ＋ **∴ 且**要求**连续两个尺寸**都快**✗（**∴ 从而**排除单点噪声 ✓）★
//       ＋ **∴ 为什么 ✗**：**本会话已实测 5 轮的乱跳**✗ ⇒ **∴ 单点**不可信** ✓ ★
// **★★★ 还必须有**显著性门槛** ✗ ★★★**（**第 793 轮；**变异检验抓到的 ✓）
//   **∴ 变异检验怎么抓到我的 ✗**：**我第一版只要求**严格更快 ＋ 连续两点**✗
//     ＋ **∴ 实测（**be15 的真实数据 ✓）**它**给出 **65536 px**✗ 而**不是 1048576** ✓
//       ＋ **∴ 根因 ✗**：** 256² 的 GPU 17 vs CPU 18**✗ ／ ** 362² 的 26 vs 27**
//         ⇒ **∴ 差 **都只有 1 ms**** ⇒ **∴ 那**在噪声内** ✓ ★★
//           ＋ **∴ 而**真正的稳定优势在 **≥1024²**✗（**∴ 差 30／55／106 ms ✓）★
//   **∴ 定义 ✗**：**「**显著更快**」＝ 比值 ≤ 0.9**✗（**∴ 即**至少快 10% ✓）
//     ＋ **∴ 并**要求**连续两点**都显著**✗ ⇒ **∴ 从而**排除单点与 1 ms 级别的抖动** ✓ ★
//       ＋ **∴ 为什么 0.9 ✗**：**本会话实测**两路等价区（**≤724² ✓）**的比值是 0.94–1.07**
//         ⇒ **∴ 0.9 **清楚地在它之外**** ✓ ★★★
const RATIO = 0.9;
const clearlyFaster = (r) => r.gpuMs !== null && r.cpuMs !== null && r.gpuMs < r.cpuMs * RATIO;
// **★★★ 不把「首个」包装成**精确交点** ✗ ★★★**（**第 793 轮；**两轮变异检验的结论 ✓）
//   **∴ 我试过两版，都被变异检验打回 ✗ ★**：
//     ⇒ **∴ v1 ✗**：**「首次 `gpuMs <= cpuMs`」** ⇒ **∴ 90² 的 9.00 ≤ 9.00（**相等 ✓）
//       ＋ **∴ 于是**报「**交点 8100 px」**✗ 而**真实在 524 K～1.05 M** ✓ ★★
//         ⇒ **∴ v2 ✗**：**「严格更快 ＋ 连续两点」** ⇒ **∴ 65536 px**（**∴ 因为** 256²／362² 只差 1 ms ✓）
//           ＋ **∴ 于是**加了显著性门槛（**ratio ≤ 0.9 ✓）⇒ **∴ 真实数据**对了**
//             ＋ **∴ 而**变异①（**1024² 改成相等 ✓）⇒ **∴ 它**跳到 2096704 px** ✗
//               ＋ **∴ 变异②（**1448² 改成接近 ✓）⇒ **∴ 它**跳到 4194304 px** ✗
//                 ⇒ **∴ 根因 ✗**：**「**连续两点**」可以是**任何**局部**** ⇒
//                   ⇒ **∴ 「**首个**」的语义**在这种判据下不可判**（**∴ 噪声能造出任意局部 ✓）★
//   **∴ 所以 ✗**：**改为**如实报出**✗：
//     ⇒ **∴ ①** **所有**显著更快的尺寸（**ratio ≤ 0.9 ✓）—— **∴ 那**是**可判的量** ✓
//       ⇒ **∴ ②** 其中最**小**的一个 ⇒ **∴ 只**说它是「**首个显著更快**」**✗
//         ＋ **∴ 并**明确标注**它**是**否**“持续”**（**后面每一个是否也都更快 ✓）★
//           ⇒ **∴ ③** **∴ 不**输出一个**看起来精确的交点**✗（**∴ 那**会误导重定阈值** ✓）★★
const faster = paired.filter(clearlyFaster);
const firstFaster = faster.length ? faster[0] : null;
const sustains = firstFaster ? paired.slice(paired.indexOf(firstFaster)).every(clearlyFaster) : false;
const cross = firstFaster;
if (cross) {
  // **★★★ 如实报出，**不**包装成精确交点 ✗ ★★★**（**第 793 轮 ✓）
  console.log(`  ★ 首个**显著更快**的尺寸 ⇒ **${cross.px} px（${cross.side}²）**` +
    `｜GPU ${cross.gpuMs.toFixed(2)} ms vs CPU ${cross.cpuMs.toFixed(2)} ms` +
    `（比 ${(cross.gpuMs / cross.cpuMs).toFixed(2)}×）`);
  console.log(`    ⇒ 判定门槛：**GPU < CPU × ${RATIO}**（**即**至少快 10% ✓ —— **∴ 不**接受「相等」✓）`);
  console.log(`    ⇒ 所有**显著更快**的尺寸（**共 ${paired.filter(clearlyFaster).length} 个 ✓）：` +
    paired.filter(clearlyFaster).map((r) => `${r.side}²`).join("／"));
  console.log(sustains
    ? "    ⇒ **它之后每一个尺寸**也都显著更快** ⇒ **∴ 可以**把它当作优势区的**下界** ✓"
    : "    ⇒ ⚠️ **它之后**并非每个尺寸都显著更快** ⇒ **∴ 那**说明该点**不稳定** ⇒ **∴ 不要**据此重定阈值** ✓");
  console.log("    ⇒ 与 `yanshi-gpu/src/lib.rs::threshold_for` 的现行值对照后决定是否重定");
  process.exit(0);
}
// **★ 末段：区分「本机答不了」与「真 GPU 确实不更快」（2026-10-15 实测修 ✓）★**
//
// **为什么** ✗：本判据要问的是"GPU 从哪个尺寸起比 CPU 快" ✓ —— 而**软件光栅器**（llvmpipe 之类 ✓）
//   或**根本拿不到真适配器名**的机器**答不了这个问题** ✗。
//   原来这里一律 `exit 1` ✗ ⇒ ⇒ 本机实测 **5 次里 4 次红、1 次绿** ✗（擦线过了 ✓）
//   ⇒ **CI 随机变红** ✗ ⇒ **真红会被淹没** ✓ —— 正是台账存在的理由 ✓。
//   本文件**上面**已经有一条正确的先例 ✓（"这不是失败，是**判据自身的前置条件缺失** ⇒ exit 3" ✓）
//   ⇒ 这里按**同一条纪律**（"**判据跑不了就退 3**" ✓）补齐 ✓。
const softwareAdapter = adapterSeen !== null && /llvmpipe|softpipe|swrast|software|zink on llvmpipe/i.test(adapterSeen);
if (!adapterSeen || softwareAdapter) {
  console.log("  ⚠ 扫完整个区间都没找到**显著更快**的尺寸 —— 但这**不是失败** ✗：");
  console.log(`    ⇒ 适配器 ＝ ${adapterSeen ? `\`${adapterSeen}\`（软件光栅器）` : "**没有拿到真名字**（只有渲染节点）"}`);
  console.log("    ⇒ **本机答不了这个问题**（软件光栅器不能当成硬件 ⇒ 第 318 轮的坑）");
  console.log("    ⇒ ⇒ 按纪律**退出 3**（判据跑不了，不是产品红）✓ —— **不要**把它登记成产品缺陷 ✗");
  process.exit(3);
}
console.log("  ✗ 扫完整个区间都没找到**显著更快**的尺寸（GPU 一直更慢，或优势不到 10%）⇒ exit 1");
console.log("    ⇒ 这条是**真结论** ✓：适配器是真的（" + adapterSeen + "）⇒ GPU 在这个区间确实没有优势 ✓");
console.log("    ⇒ 与 `yanshi-gpu/src/lib.rs::threshold_for` 的现行值对照后决定是否重定 ✓");
process.exit(1);
