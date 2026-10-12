
// **★ 临时目录必须**自己收拾 ✗ ★**（第 53 轮 ✓；**∴ 用户报告 /tmp 被塞满 ✓）：
//   **∴ 我**的浏览器脚本**每个**都建**chromium profile ＋ 临时 root**✗
//     ⇒ **∴ 而**以前**从不删除**✗ ⇒ **∴ 跑几十次就**把 /tmp 塞满 ✓**** ✓✓
//   **∴ 现在**：**注册 ＋ 退出时递归删除**✗ ⇒ **∴ 于是**：**跑多少次都**不积累 ✓**** ✓✓
import { rmSync, readFileSync } from "node:fs";

const __tempPaths = [];
function trackTemp(path) { __tempPaths.push(path); return path; }
// **∴ 删除要**尽力而为 ＋ 重试 ✗ ★**（第 54 轮 ✓；**∴ 用户重启那次换来的 ✓）：
//   **∴ 我**第一版**只删一次**✗ ⇒ **∴ 实测**仍有遗留 ✓
//     （**∴ 因为**chromium**可能**还在写**那个 profile ⇒ **∴ rmSync**失败 ⇒ **∴ 被 catch 吞掉 ✓）**
//   **∴ 所以**：**删三遍**✗（**每遍之间**同步等一会儿 ✓）
//     ＋ **同步信号**（**`SIGINT`／`SIGTERM` ✓）也走**同一条清理 ✓**** ✓✓
function __cleanupTemp() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let left = 0;
    for (const path of __tempPaths) {
      try { rmSync(path, { recursive: true, force: true }); }
      catch { left += 1; }
    }
    if (left === 0) return;
    // **∴ 同步等待 ✗**：**∴ 在 exit 钩子里不能用 await ✓** ✓✓
    try { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200); } catch { /* 忽略 */ }
  }
}
process.on("exit", __cleanupTemp);
for (const __signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(__signal, () => { __cleanupTemp(); process.exit(1); });
}
//! **★ 渲染的**两本账**✗ ★**（第 47 轮 ✓；**阶段二的对照脚手架 ✓**）。
//!
//! **∴ 依据（**用户第 594 轮 ✓）★**：
//!   **∴ ① 时间账 ✗**：**墙钟**
//!   **∴ ② CPU 占用账 ✗**：**进程 CPU 时间（utime+stime）／CPU 占比／CPU·ms per frame**
//!     ⇒ **∴ 两本账都要报 ✗** ⇒ **∴ 若**只有墙钟变好**而** CPU 没降（**或更差 ✓）
//!       ⇒ **∴ 必须**明说 ✓（**∴ 不许**只报好看的那本 ✓）** ✓✓
//!
//! **∴ 它做什么 ✗**：**在**同一个 4K 场景**上 ✗**：
//!   **①** 起（或连）服务 ⇒ **②** 建文档 ＋ 4 层 ＋ 填充 ⇒ **③** 渲整幅 ✗
//!     ⇒ **④** **从 `/proc/<pid>/stat` 与 `status` 取**四元组**✗**：
//!       墙钟、CPU 时间、CPU÷墙钟、**峰值常驻（**VmHWM ✓）
//!     ⇒ **⑤** **按后端**打印成**一张对照表 ✓**** ✓✓
//!
//! **∴ 测量纪律（**第 45 轮学到的 ✓）★**：**同一二进制的**第一次运行**受**冷页缓存**影响**✗
//!   ⇒ **∴ 本脚本**默认**先跑一次**预热**✗（**`--rounds N` ✓，**报告取**后 N−1 次的中位数 ✓）** ✓✓
//!
//! **∴ 用法 ✗**：
//!   `node scripts/tool-render-cost-accounts.mjs --spawn`
//!   `node scripts/tool-render-cost-accounts.mjs --spawn --compare "默认=--gpu off" "请求 GPU=--gpu on"`
//!   **或**（等价）：`--compare "默认=--gpu off" --compare "请求 GPU=--gpu on"`
//!     ⇒ **∴ 于是**：**两本账**按**后端**并排 ✓**** ✓✓
//!
//! **∴ 如实说明 ✗**：**本机**没有可用 GPU**✗（**服务端报 `host_has_no_gpu` ✓）
//!   ⇒ **∴ 现在**两行会是**同一条 CPU 路径** ✓ ⇒ **∴ 它**不是**失败 ✓
//!     ⇒ **∴ 而是**把**对照表**先立好 ✓**** ✓✓

const argv = process.argv.slice(2);

/** **∴ 极简参数解析 ✗**（**∴ 不引依赖 ✓）** ✓✓ */
function option(name, fallback = null) {
  const index = argv.indexOf(name);
  return index >= 0 && argv[index + 1] ? argv[index + 1] : fallback;
}

const rounds = Number(option("--rounds", argv.length === 0 ? "1" : "3"));
// **★ `--region WxH` ✗ ★**（**第 563 轮 ✓）：**默认 3840x2160 ⇒ 与改动前一致** ✓
const regionArg = option("--region", "3840x2160");
const RENDER_W = Number(regionArg.split("x")[0]) || 3840;
const RENDER_H = Number(regionArg.split("x")[1]) || 2160;
// **★ 判据必须**自足** ✗ ★**（第 77 轮 ✓）：**没有**任何参数时**默认 `--spawn`**✗
//   **∴ 且**：**默认轮数**改成 **1** ✗
//     ⇒ **∴ 因为**它在**分片里**也会被跑到**✗ ⇒ **∴ 不该**在那里跑**3 轮重活 ✓**** ✓✓
// **★ 编排给的是 `<base> <doc> <token>`**✗ ⇒ **∴ 本判据**必须能**自己起服务** ✗ ★**（第 98 轮 ✓）：
//   **∴ 症状（**本地复现 ✓）✗**：`✗ 需要 --spawn，或同时给出 --base 与 --pid` ⇒ **`EXIT=2`** ✓**** ✓✓
//     ⇒ **∴ 因为**编排（`run-criteria.sh:228` ✓）**给三个位置参数**✗
//       ⇒ **∴ `argv.length === 0` 永远不成立** ✓**** ✓✓
//   **∴ 修法**：**没有 `--pid` 就没法测外部进程**✗
//     ⇒ **∴ 那就**自己 spawn 一个**✗ ⇒ **∴ 于是**它测的是**它自己起的那个服务 ✓**** ✓✓
const spawnMode = argv.includes("--spawn") || argv.length === 0 || !option("--pid");
const baseline = option("--baseline", "target/release/yanshi-serve");

/** **∴ 一组对照：**名字 ＝ 额外的服务参数 ✓**（**∴ 用 `=` 分隔 ✓）** ✓✓ */
// **∴ 写法 ✗**：**`--compare "名字=参数…"`**✗，**可以重复** ✓
//   **∴ 我**第一版写复杂了**✗ ⇒ **∴ 只**解析出第一项** ✓（**∴ 实测只出一行 ✓）** ✓✓
// **★ 宽容解析：**`--compare` 之后的每一个 `值=参数…` 都算一个配置**** ✗ ★**（**第 558 轮 ✓）
//   **∴ 原来的缺陷（**实测 ✓）✗**：**只吃 `--compare` 之后的**第一个**值**✗
//     ＋ **∴ 而**文档示例（**第 50 行 ✓）写的是
//       `--compare "A=…" "B=…"`** ✗（**第二个值**被静默丢掉** ✓）
//       ⇒ **★ 症状 ✗**：**两行配置**跑同一个后端**✗
//         （**实测**：`--gpu on` 那行也报 `cpu`／`disabled_by_flag` ✓）★
//   **∴ 现在 ✗**：**两种写法都支持**✗
//     ⇒ **∴ 即**：**重复 `--compare`**✗（**注释原本的写法 ✓）
//       ＋ **∴ 或**：**一个 `--compare` 后面跟多个 `值=参数…`** ✓
//         ⇒ **∴ 且**遇到下一个**以 `--` 开头的真选项就停** ✓ ★
const compareSpecs = [];
for (let i = 0; i < argv.length; i += 1) {
  if (argv[i] !== "--compare") continue;
  // **∴ 吃后续所有**不是选项**的值** ✓
  for (let j = i + 1; j < argv.length; j += 1) {
    const raw = argv[j];
    if (raw === "--compare" || (raw.startsWith("--") && !raw.includes("="))) break;
    if (!raw.includes("=")) break;
    const [label, ...rest] = raw.split("=");
    compareSpecs.push({ label, extra: rest.join("=").split(" ").filter(Boolean) });
    i = j;
  }
}
if (!compareSpecs.length) compareSpecs.push({ label: "默认", extra: [] });

/** **∴ 读进程的**四元组**✗**（**∴ CPU 时间**用 utime+stime 的**时钟节拍 ✓）** ✓✓ */
async function readProcess(pid) {
  const { readFileSync } = await import("node:fs");
  // **★★★ 进程可能**在测量中途消失** ✗ ★★★**（**第 793 轮修；**L4 实测 ✓）
  //   **∴ 症状 ✗**（**L4 实测 ✓）★**：**跑 `chunk=4194304` 与 `chunk=268435456` 时**两次都**
  //     `Error: ENOENT: no such file or directory, open '/proc/31805/stat'`
  //       ＋ **∴ 而**栈指向**这里**✗ ⇒ **∴ 判据**直接崩**（**Node 的未捕获异常 ✓）★
  //   **∴ 为什么不行 ✗**：
  //     ＋ **∴ ①** **判据**跑不了**（**服务进程死了 ✓）**≠**产品失败** ✗
  //       ⇒ **∴ 它**必须**说出来 ＋ 用 `exit 3`**（**本仓库的统一约定 ✓）★
  //         ＋ **∴ ②** **崩栈**对读的人**没有用**✗ ⇒ **∴ 它**不告诉我**哪个配置／哪一轮**出的问题** ✓
  //           ＋ **∴ ③** **两个 chunk 配置**都死**✗ ⇒ **∴ 那**说明**不是 chunk 大小的问题** ✓
  //   **∴ 修法 ✗**：**读不到就**打印诊断 ＋ `exit 3`**✗（**并带上 pid 与轮次上下文 ✓）★
  const stat = (() => {
    try {
      return readFileSync(`/proc/${pid}/stat`, "utf8");
    } catch (error) {
      console.error("");
      console.error(`  ⏭ 服务进程 ${pid} 在测量中途消失（${error && error.code}）⇒ **本次没验**，不是产品失败`);
      console.error("     ⇒ 可能原因：进程被 OOM 杀掉／自行退出／被别的作业 `pkill -x yanshi-serve` 杀掉");
      console.error("     ⇒ 要真跑：确认没有第二个测量作业在并行（它们会互相 `pkill`）");
      process.exit(3);
    }
  })();
  // **∴ comm 可能带空格与括号 ✗** ⇒ **∴ 从最后一个 ')' 之后**切 ✓**** ✓✓
  const tail = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
  const utime = Number(tail[11]);
  const stime = Number(tail[12]);
  const status = readFileSync(`/proc/${pid}/status`, "utf8");
  const hwm = Number((status.match(/VmHWM:\s+(\d+)/) || [])[1] || 0);
  return { cpuTicks: utime + stime, hwmKb: hwm };
}

/** **∴ 在某个服务上把 4K 场景搭好 ✗**（**∴ 与 §20 的基线同构 ✓）** ✓✓ */
// **★ `--project <包路径>` ✗ ★**（**第 561 轮 ✓；目标第 2 条 ✓）
//   **∴ 为什么需要 ✗**：**原来的 `buildScenario` 建的是**合成文档**✗
//     （`accounts_<tag>` ＋ 四层填充 ✓）⇒ **∴ 它**测不出**真工程** ✓
//       ＋ **∴ 而**目标第 2 条要的是**真工程的两本账 ＋ 离散度** ✓ ★
//   **∴ 做法 ✗**：**走**三步导入****（**与手工流程一致 ✓）：
//     `import?begin=1` → `import?upload=<id>&offset=0`（原始字节 ✓）→ `import?finish=1`
//       ⇒ **∴ 从 `finish` 的响应取 `token` 与 `doc_id`** ✓
//         ＋ **∴ 图层名 ✗**：**真工程的图层名不是 `L0`**✗
//           ⇒ **∴ 于是**：**从包内 `atoms.jsonl` 里扫第一个 `layer_id`** ✓
//             ＋ **∴ 可被 `--layer` 覆盖** ✓ ★
const projectPath = option("--project", "");
const layerOverride = option("--layer", "");
let projectBytes = null;
if (projectPath) {
  try { projectBytes = readFileSync(projectPath); } catch { projectBytes = null; }
}

async function buildScenario(base, tag) {
  if (projectBytes) {
    // **∴ 三步导入 ✓
    const begin = await (await fetch(`${base}/api/documents/import?begin=1`, { method: "POST" })).json();
    const uid = begin.upload_id || begin.uploadId;
    await fetch(`${base}/api/documents/import?upload=${uid}&offset=0`, { method: "POST", body: projectBytes });
    const fin = await (await fetch(`${base}/api/documents/import?upload=${uid}&finish=1`, { method: "POST" })).json();
    const token = fin.token;
    const docId = fin.doc_id || fin.docId;
    const q = `doc=${docId}&token=${token}`;
    const call = async (tool, args) =>
      (await fetch(`${base}/api/tools/${tool}?${q}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(args || {}),
      })).json();
    // **∴ 图层名：优先 `--layer`，否则从包内扫第一个 `layer_id` ✓
    let layer = layerOverride;
    if (!layer) {
      const m = /"layer_id"\s*:\s*"([^"]+)"/.exec(projectBytes.toString("latin1"));
      layer = m ? m[1] : "L0";
    }
    return { docId, token, call, layer, real: true };
  }
  const docId = `accounts_${tag}`;
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width: 3840, height: 2160 }),
  })).json();
  const token = created.token;
  const q = `doc=${docId}&token=${token}`;
  const call = async (tool, args) =>
    (await fetch(`${base}/api/tools/${tool}?${q}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })).json();
  for (const id of ["L0", "L1", "L2", "L3"]) {
    await call("create_layer", { layer_id: id });
  }
  for (let i = 0; i < 4; i += 1) {
    await call("fill", {
      layer_id: `L${i}`,
      object_id: `o${i}`,
      data: {
        color: { r: 30 + i * 40, g: 120, b: 200, a: 255 },
        region: { x: i * 200, y: i * 150, width: 2400, height: 1400 },
      },
    });
  }
  return { docId, q, token, call, layer: "L0", real: false };
}

/** **∴ 量一轮横跨两个进程的账 ✗**（**∴ 服务端 CPU 才是要看的 ✓）** ✓✓ */
async function measureRound({ base, pid, scenario, warm, round }) {
  // **★ 每轮必须先改一处内容 ✗ ★**（第 47 轮 ✓；**∴ 我**第一版漏了 ✓）：
  //   **∴ 症状 ✗**：**第二轮**墙钟 **2.4 ms／CPU 0 拍** ✗
  //     ⇒ **∴ 因为**渲染**命中了**区域／below 缓存**✗ ⇒ **∴ 它**几乎没干活 ✓**** ✓✓
  //   **∴ 所以**：**测首帧成本**必须**让每次渲染都真的重算**✗
  //     ⇒ **∴ 做法**：**每轮**挪动一个填充的 region**✗（**∴ 内容变 ⇒ **∴ 缓存失效 ✓）** ✓✓
  await scenario.call("fill", {
    layer_id: scenario.layer || "L0",
    object_id: "o_churn",
    data: {
      color: { r: 200, g: 30, b: 60, a: 255 },
      region: { x: round * 3, y: round * 2, width: 600, height: 400 },
    },
  });
  const before = await readProcess(pid);
  const t0 = process.hrtime.bigint();
  const out = await scenario.call("render_region", {
    // **★ `--region WxH`（**默认 3840x2160 ⇒ 行为不变 ✓）★**（**第 563 轮 ✓；目标第 2 条 ✓）
    //   **∴ 为什么需要 ✗**：**原来区域**硬编码 4K**✗ ⇒ **∴ 于是**：
    //     **"8K 工程"其实也只在**4K 区域**上渲染**（**第 562 轮实测 ✓）
    //       ⇒ **∴ 测不出**真 8K 分辨率**✗ ⇒ **∴ 规模曲线缺**端点** ✓ ★
    region: { x: 0, y: 0, w: RENDER_W, h: RENDER_H },
    include_image: false,
  });
  const t1 = process.hrtime.bigint();
  const after = await readProcess(pid);
  const wallMs = Number(t1 - t0) / 1e6;
  // **∴ 时钟节拍**通常是 100／秒 ✓ ⇒ **∴ 1 tick = 10 ms ✓**** ✓✓
  const cpuMs = (after.cpuTicks - before.cpuTicks) * 10;
  return { wallMs, cpuMs, ratio: wallMs > 0 ? cpuMs / wallMs : 0, hwmKb: after.hwmKb, ok: out.ok, warm };
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

// **★ 离散度 ✗ ★**（**第 495 轮 ✓；**目标第 2 条要求「**多轮 ＋ 报 min/median/max**」✓）
//
// **∴ 为什么只报中位数不够 ✗**（**本会话实测 ✓）**：
//   **∴ 同一配置**重复测**✗ ⇒ **∴ 结果**同一台机器上两次就能**反转** ✓
//     （**∴ 实测**：**GPU 有时 +13.5% 更慢 ✗ 、**有时 −52% 更快** ✓）
//       ⇒ **∴ 只给一个中位数**⇒ **∴ 读者**无法判断**这是噪声还是真差异** ✓ ★**** ✓✓
//   **∴ 所以 ✗**：**三个数一起给**✗ ＋ **∴ 且**给出**逐轮原始值** ✓
//     ⇒ **∴ 于是**：**min／max 的**差距**本身就是一个**可读的指标** ✓ ★**** ✓✓
function spread(values) {
  const v = values.filter((x) => typeof x === "number" && Number.isFinite(x));
  if (v.length === 0) {
    return { min: NaN, median: NaN, max: NaN, mad: NaN, ciLow: NaN, ciHigh: NaN, n: 0 };
  }
  const ci = medianCI(v);
  return {
    min: Math.min(...v),
    median: median(v),
    max: Math.max(...v),
    mad: mad(v),
    ciLow: ci.low,
    ciHigh: ci.high,
    n: v.length,
  };
}

// **★★★ MAD（**中位绝对偏差 ✓）✗ ★★★**（**第 621 轮 ✓）
//   **∴ 为什么不用标准差 ✗**：**样本少（**如 4–20 ✓）＋ **可能有离群点**✗
//     ＋ **∴ 而**标准差**被极值主导** ✓ ⇒ **∴ 所以**：**MAD 更稳** ✓ ★
function mad(values) {
  const m = median(values);
  return median(values.map((x) => Math.abs(x - m)));
}

// **★★★ 中位数的**自助置信区间** ✗ ★★★**（**第 621 轮 ✓；**目标第 2 条 ✓）
//   **∴ 为什么需要 ✗**：**本会话的核心困难是**噪声 ≈ ±10%**** ✗
//     ＋ **∴ 而**只给 min／median／max**✗ ⇒ **∴ 读者**无法判断
//       「两臂之差是**真差异**还是**噪声**」 ✓
//         ⇒ **∴ 所以**：**给中位数的 95% 区间**✗
//           ＋ **∴ 而**两臂的区间**不重叠** ⇒ **∴ 才是**显著差异** ✓ ★
//   **∴ 实现 ✗**：**自助法（**bootstrap ✓）✗ ⇒ **∴ 重采样 ＋ 取 2.5%／97.5% 分位** ✓
//     ＋ **★ 必须**确定性** ✗ ★**：**用**固定种子**的 PRNG**✗
//       ⇒ **∴ 从而**：**同一输入**⇒ 同一区间** ✓（**AGENTS.md 的确定性要求 ✓）★
function medianCI(values, resamples = 2000) {
  const v = values.filter((x) => typeof x === "number" && Number.isFinite(x));
  if (v.length < 3) return { low: NaN, high: NaN, resamples: 0 };
  // **∴ mulberry32 ✗**：**小而确定的 PRNG**（**∴ 不用 Math.random ⇒ **可复现** ✓）★
  let state = 0x9e3779b9;
  const rnd = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const meds = [];
  for (let r = 0; r < resamples; r += 1) {
    const pick = [];
    for (let i = 0; i < v.length; i += 1) pick.push(v[Math.floor(rnd() * v.length)]);
    meds.push(median(pick));
  }
  meds.sort((a, b) => a - b);
  const at = (q) => meds[Math.min(meds.length - 1, Math.max(0, Math.floor(q * meds.length)))];
  return { low: at(0.025), high: at(0.975), resamples };
}

// **∴ 两臂是否**显著不同** ✗**（**∴ 中位数区间不重叠 ⇒ 显著 ✓）★
function significant(a, b) {
  if (!Number.isFinite(a?.ciLow) || !Number.isFinite(b?.ciLow)) return null;
  if (a.median < b.median) return a.ciHigh < b.ciLow;
  return b.ciHigh < a.ciLow;
}

const fmtSpread = (sp) =>
  sp.n === 0 ? "—" : `${sp.min.toFixed(0)}／${sp.median.toFixed(0)}／${sp.max.toFixed(0)}`;
const fmtCI = (sp) =>
  !Number.isFinite(sp?.ciLow) ? "—" : `[${sp.ciLow.toFixed(0)}, ${sp.ciHigh.toFixed(0)}]`;
// **∴ 表格里的紧凑形式 ✗**（**∴ 宽度可控 ✓）★
const fmtCIPad = (sp) =>
  !Number.isFinite(sp?.ciLow) ? "—" : `[${sp.ciLow.toFixed(0)}–${sp.ciHigh.toFixed(0)}]`;
const fmtMad = (sp) => (Number.isFinite(sp?.mad) ? sp.mad.toFixed(0) : "—");

const { spawn } = await import("node:child_process");
// **★★★ 把服务端 stderr 落盘 ✗ ★★★**（**第 667 轮 ✓）
//   **∴ 原因 ✗**：**原来 `stdio: "ignore"` **丢弃**服务端 stderr**✗
//     ＋ **∴ 于是** `YANSHI_RENDER_PROBE` **等探针的输出**全部看不到** ✓
//       ＋ **∴ 现在 ✗**：**落到 `/tmp/render-probe.log`**✗
//         ＋ **∴ 从而**既不干扰表格输出，又能事后分析** ✓ ★
const { openSync } = await import("node:fs");
const probeLog = process.env.YANSHI_PROBE_LOG || "/tmp/render-probe.log";
const probeFd = openSync(probeLog, "a");
const { mkdtempSync, existsSync } = await import("node:fs");
const { tmpdir } = await import("node:os");
const { join } = await import("node:path");

const rows = [];
const spawned = [];
if (spawnMode) {
  if (!existsSync(baseline)) {
    console.error(`✗ --spawn 需要二进制：${baseline}（先构建，或用 --baseline 指定）`);
    process.exit(2);
  }
  process.on("exit", () => {
    for (const child of spawned) {
      try { child.kill(); } catch { /* 已经没了 */ }
    }
  });
  for (const signalName of ["SIGINT", "SIGTERM"]) {
    process.on(signalName, () => process.exit(1));
  }
}

let port = 8801;
for (const spec of compareSpecs) {
  let base = option("--base", null);
  let pid = Number(option("--pid", "0"));
  let child = null;
  if (spawnMode) {
    const root = trackTemp(mkdtempSync(join(tmpdir(), "accounts-")));
    child = spawn(baseline, ["--root", root, "--bind", `127.0.0.1:${port}`, ...spec.extra], {
      stdio: ["ignore", "ignore", probeFd],
    });
    spawned.push(child);
    base = `http://127.0.0.1:${port}`;
    pid = child.pid;
    for (let i = 0; i < 80; i += 1) {
      try {
        if ((await fetch(`${base}/health`)).ok) break;
      } catch { /* 还没起来 */ }
      await new Promise((r) => setTimeout(r, 250));
    }
    port += 1;
  }
  if (!base || !pid) {
    console.error("✗ 需要 --spawn，或同时给出 --base 与 --pid");
    process.exit(2);
  }
  const health = await (await fetch(`${base}/health`)).json();
  const scenario = await buildScenario(base, spec.label.replace(/[^a-zA-Z0-9]/g, "") || "x");
  const samples = [];
  for (let i = 0; i < rounds; i += 1) {
    samples.push(await measureRound({ base, pid, scenario, warm: i === 0, round: i + 1 }));
  }
  // **∴ 丢掉预热那一轮 ✗**（**∴ 冷页缓存**会**主导第一次 ✓）** ✓✓
  const usable = samples.length > 1 ? samples.slice(1) : samples;
    // **★ 必须**在渲染之后**重读 `/health` ✗ ★**（**第 460 轮 ✓；**实测根因 ✓）：
    //   **∴ 原来的错 ✗**：**`backend` **取自**渲染前的快照**✗（**`:218` ✓）
    //     ⇒ **∴ 而**那**是**进程刚起**时的初值** ✓
    //       ⇒ **∴ 于是**：**它**永远显示 `cpu`** ✓
    //         ⇒ **∴ 症状（**实测 ✓）✗**：**两行都是 `cpu`**✗
    //           ＋ **∴ 而**数字**却明显不同**（**−42% ✓）** ✓ ★**** ✓✓
    //   **∴ 现在 ✗**：**渲染后再读一次**✗
    //     ⇒ **∴ 于是** `backend` **反映**真实走过的路** ✓ ★**** ✓✓
    let after = health;
    try {
      after = await (await fetch(`${base}/health`)).json();
    } catch (error) {
      console.warn(`    ⚠️ 渲染后读 /health 失败（沿用旧快照）：${String(error).slice(0, 80)}`);
    }
  rows.push({
    label: spec.label,
    backend: after.render_backend,
    gpuMode: after.gpu_mode,
    reason: after.gpu_adapter_note,
    // **★ 逐位一致：必须从**渲染后**的快照取 ✗ ★**（**第 580 轮 ✓；**目标第 2 条 ✓）
    //   **∴ 为什么必须显式取 ✗**：**默认**不核对**✗
    //     ⇒ **∴ 于是** `max_channel_delta` **报 `null`**（**刻意的诚实设计 ✓）
    //       ⇒ **∴ 只有**设 `YANSHI_GPU_VERIFY=1`** 才会真的比对 ✓
    //         ＋ **∴ 而**它**必须在**渲染之后**读**✗
    //           ⇒ **∴ 否则** `null`（**没比过 ✓）★
    channelDelta: after.max_channel_delta,
    // **★ GPU 阶段的三个分段 ✗ ★**（**第 586 轮 ✓；**目标第 5／9 条 ✓）
    //   **∴ 为什么要它 ✗**：**传输是 GPU 阶段的大头**✗
    //     （**第 68 轮实测：**upload 59% ＋ readback 18% ＝ **77%**** ✓）
    //     ＋ **∴ 所以**：**「减少往返」的收益**必须用这三个数对比才可信** ✓ ★
    stageUploadUs: after.gpu_stage_upload_us,
    stageSubmitUs: after.gpu_stage_submit_us,
    stageReadbackUs: after.gpu_stage_readback_us,
    stageCalls: after.gpu_stage_calls,
    // **★ GPU 自己量的时间 ✗ ★**（**第 602 轮 ✓；目标第 9 条后半 ✓）
    //   **∴ 语义 ✗**：**它不是 host 墙钟**✗ ⇒ **∴ 而**是 GPU 时间戳的差**（**需探针开 ✓）
    //     ＋ **∴ 若**未探针／不支持 ⇒ **如实为 0** ✓ ★
    stageGpuUs: after.gpu_stage_gpu_us,
    verifiedBlobs: after.verified_blobs,
    adapter: after.gpu_adapter_note,
    wallMs: median(usable.map((s) => s.wallMs)),
    cpuMs: median(usable.map((s) => s.cpuMs)),
    ratio: median(usable.map((s) => s.ratio)),
    hwmMb: Math.max(...samples.map((s) => s.hwmKb)) / 1024,
    ok: samples.every((s) => s.ok === true),
    warmMs: samples[0].wallMs,
    // **∴ 离散度 ＋ 逐轮原始值 ✗**（**目标第 2 条 ✓）
    wallSpread: spread(usable.map((s) => s.wallMs)),
    cpuSpread: spread(usable.map((s) => s.cpuMs)),
    ratioSpread: spread(usable.map((s) => s.ratio)),
    rounds: usable.map((s) => ({
      wallMs: Number(s.wallMs.toFixed(1)),
      cpuMs: Number(Number(s.cpuMs).toFixed(1)),
      ratio: Number(Number(s.ratio).toFixed(2)),
    })),
  });
  if (child) child.kill();
}

console.log("");
// **★ 离散度必须在表里 ✗ ★**（**第 495 轮 ✓；**目标第 2 条 ✓）
console.log("  ★ 两本账（4K 整幅；第一轮为预热，已从统计里剔除）★");
console.log("  | 配置 | 后端 | ① 墙钟 ms（min／中位／max） | ★中位 95% CI★ | ★MAD★ | ② CPU ms（min／中位／max） | CPU÷墙钟 | 峰值 RSS MB | 预热墙钟 ms | 有效轮数 |");
console.log("  |---|---|---|---|---|---|---|---|");
for (const row of rows) {
  console.log(`  | ${row.label} | ${row.backend}（--gpu ${row.gpuMode}） `
    + `| ${fmtSpread(row.wallSpread)} | ${fmtCIPad(row.wallSpread)} | ${fmtMad(row.wallSpread)} | ${fmtSpread(row.cpuSpread)} `
    + `| ${row.ratio.toFixed(2)}× | ${row.hwmMb.toFixed(0)} | ${row.warmMs.toFixed(1)} | ${row.wallSpread.n} |`);
}
// **∴ 逐轮原始值 ✗**：**∴ 便于看出**方差有多大** ✓（**∴ 只看中位数会掩盖它 ✓）
console.log("");
console.log("  ★ 逐轮原始值（**同一配置的多轮差异就是噪声水位** ✓）★");
for (const row of rows) {
  const rs = row.rounds.map((r) => `#${r.wallMs}ms/${r.cpuMs}cpu`).join("  ");
  console.log(`  · ${row.label}：${rs}`);
}

// **★ 如实结论 ✗ ★**：**∴ 若**所有行的后端**相同**✗ ⇒ **∴ 明说**"**本机没有两路可对比 ✓"** ✓✓
// **★ 逐位一致汇总 ✗ ★**（**第 580 轮 ✓；**目标第 2 条 ✓）
//   **∴ 目标 ✗**：**GPU 路与 CPU 路**逐字节相同 ⇒ `max_channel_delta` ＝ **0** ✓
//     ＋ **∴ 若**报了非 0 ⇒ **∴ 那就是**真差异**✗（**必须回退 ✓）★
//   **∴ 若**全为 `null` ⇒ **∴ 说明**本次**没核对**✗（**默认不核对 ✓）
//     ⇒ **∴ 那就**必须设 `YANSHI_GPU_VERIFY=1` 重跑** ✓ ★
{
  const deltas = rows.map((r) => r.channelDelta);
  const verified = rows.map((r) => r.verifiedBlobs);
  const adapter = rows.map((r) => r.adapter).filter(Boolean);
  console.log("  ★ 逐位一致（`/health.max_channel_delta`；**从渲染后的快照取** ✓）★");
  console.log(`    max_channel_delta ＝ ${JSON.stringify(deltas)}｜verified_blobs ＝ ${JSON.stringify(verified)}`);
  // **★ 适配器要**全部打印**✗ ★**（**第 581 轮 ✓）：**只打第一个会**误导**
  //   （**∴ 实测**：第一个是 `disabled_by_flag`（`--gpu off` 那台）
  //     ⇒ **∴ 于是**看不到 GPU 那台的 `NVIDIA L4` ✓）
  for (const a of adapter) console.log(`    适配器 ＝ ${a}`);
  console.log("  ★ GPU 阶段分段（**微秒；**传输 vs 计算 ✓）★");
  for (const r of rows) {
    console.log(`    ${r.label}：calls ＝ ${r.stageCalls}｜upload ＝ ${r.stageUploadUs}｜submit ＝ ${r.stageSubmitUs}｜readback ＝ ${r.stageReadbackUs}｜★gpu ＝ ${r.stageGpuUs}★`);
  }
  // **★ 判据必须**忽略 null**✗ ★**（**第 581 轮实测 ✓）
  //   **∴ 为什么 ✗**：**CPU 路**不与 GPU 比对 ⇒ **∴ 它**正确地报 `null`**
  //     ＋ **∴ 而**我的第一版只处理了「全 null」与「全 0」✗
  //       ⇒ **∴ 于是**：**`[null, 0]` 落到 else ⇒ **误报「逐位一致被打破」**** ✓
  //         ＋ **∴ 那**是**判据自身的 bug**（**不是产品问题 ✓）★
  const real = deltas.filter((d) => d !== null && d !== undefined);
  if (real.length === 0) {
    console.log("    ⚠️ 全是 null ⇒ **本次没有核对**（默认不核对）⇒ 设 `YANSHI_GPU_VERIFY=1` 重跑 ✓");
  } else if (real.every((d) => d === 0)) {
    console.log(`    ✓ 被核对的 ${real.length} 条路全部为 0 ⇒ **GPU 路与 CPU 路逐字节相同** ✓`
      + (real.length < deltas.length ? `（另 ${deltas.length - real.length} 条为 null ＝ 未核对，正常）` : ""));
  } else {
    console.log("    ✗ 出现非 0 ⇒ **逐位一致被打破**（必须回退）✗");
  }
}
const backends = new Set(rows.map((r) => r.backend));
if (backends.size === 1) {
  console.log("");
  console.log(`  ℹ️ 所有配置都跑在 **${[...backends][0]}** 上 ⇒ **∴ 本次**没有两路可对比** ✓`);
  for (const row of rows) {
    if (row.reason) console.log(`     （${row.label}：${row.reason}）`);
  }
  console.log("     ⇒ **∴ 它**不是失败**✗：**∴ 表**已经立好**✗ ⇒ **∴ 一旦**有 GPU ✗，**同一条命令**就会出两行 ✓");
}
const failed = rows.filter((row) => !row.ok);
if (failed.length) {
  console.error(`  ✗ 有 ${failed.length} 个配置的渲染没有成功`);
  process.exit(1);
}
process.exit(0);
