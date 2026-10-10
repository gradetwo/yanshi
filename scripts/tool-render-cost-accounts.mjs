
// **★ 临时目录必须**自己收拾 ✗ ★**（第 53 轮 ✓；**∴ 用户报告 /tmp 被塞满 ✓）：
//   **∴ 我**的浏览器脚本**每个**都建**chromium profile ＋ 临时 root**✗
//     ⇒ **∴ 而**以前**从不删除**✗ ⇒ **∴ 跑几十次就**把 /tmp 塞满 ✓**** ✓✓
//   **∴ 现在**：**注册 ＋ 退出时递归删除**✗ ⇒ **∴ 于是**：**跑多少次都**不积累 ✓**** ✓✓
import { rmSync } from "node:fs";

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
//!   `node scripts/tool-render-cost-accounts.mjs --spawn --compare "默认=--gpu off" "--请求 GPU=--gpu on"`
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
const compareSpecs = [];
for (let i = 0; i < argv.length; i += 1) {
  if (argv[i] !== "--compare") continue;
  const raw = argv[i + 1] || "";
  const [label, ...rest] = raw.split("=");
  compareSpecs.push({ label, extra: rest.join("=").split(" ").filter(Boolean) });
  i += 1;
}
if (!compareSpecs.length) compareSpecs.push({ label: "默认", extra: [] });

/** **∴ 读进程的**四元组**✗**（**∴ CPU 时间**用 utime+stime 的**时钟节拍 ✓）** ✓✓ */
async function readProcess(pid) {
  const { readFileSync } = await import("node:fs");
  const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
  // **∴ comm 可能带空格与括号 ✗** ⇒ **∴ 从最后一个 ')' 之后**切 ✓**** ✓✓
  const tail = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
  const utime = Number(tail[11]);
  const stime = Number(tail[12]);
  const status = readFileSync(`/proc/${pid}/status`, "utf8");
  const hwm = Number((status.match(/VmHWM:\s+(\d+)/) || [])[1] || 0);
  return { cpuTicks: utime + stime, hwmKb: hwm };
}

/** **∴ 在某个服务上把 4K 场景搭好 ✗**（**∴ 与 §20 的基线同构 ✓）** ✓✓ */
async function buildScenario(base, tag) {
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
  return { docId, q, call };
}

/** **∴ 量一轮横跨两个进程的账 ✗**（**∴ 服务端 CPU 才是要看的 ✓）** ✓✓ */
async function measureRound({ base, pid, scenario, warm, round }) {
  // **★ 每轮必须先改一处内容 ✗ ★**（第 47 轮 ✓；**∴ 我**第一版漏了 ✓）：
  //   **∴ 症状 ✗**：**第二轮**墙钟 **2.4 ms／CPU 0 拍** ✗
  //     ⇒ **∴ 因为**渲染**命中了**区域／below 缓存**✗ ⇒ **∴ 它**几乎没干活 ✓**** ✓✓
  //   **∴ 所以**：**测首帧成本**必须**让每次渲染都真的重算**✗
  //     ⇒ **∴ 做法**：**每轮**挪动一个填充的 region**✗（**∴ 内容变 ⇒ **∴ 缓存失效 ✓）** ✓✓
  await scenario.call("fill", {
    layer_id: "L0",
    object_id: "o_churn",
    data: {
      color: { r: 200, g: 30, b: 60, a: 255 },
      region: { x: round * 3, y: round * 2, width: 600, height: 400 },
    },
  });
  const before = await readProcess(pid);
  const t0 = process.hrtime.bigint();
  const out = await scenario.call("render_region", {
    region: { x: 0, y: 0, w: 3840, h: 2160 },
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

const { spawn } = await import("node:child_process");
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
      stdio: "ignore",
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
    wallMs: median(usable.map((s) => s.wallMs)),
    cpuMs: median(usable.map((s) => s.cpuMs)),
    ratio: median(usable.map((s) => s.ratio)),
    hwmMb: Math.max(...samples.map((s) => s.hwmKb)) / 1024,
    ok: samples.every((s) => s.ok === true),
    warmMs: samples[0].wallMs,
  });
  if (child) child.kill();
}

console.log("");
console.log("  ★ 两本账（4K 整幅，中位数；第一轮为预热，已从统计里剔除）★");
console.log("  | 配置 | 后端 | ① 墙钟 ms | ② CPU ms | CPU÷墙钟 | 峰值 RSS MB | 预热墙钟 ms |");
console.log("  |---|---|---|---|---|---|---|");
for (const row of rows) {
  console.log(`  | ${row.label} | ${row.backend}（--gpu ${row.gpuMode}） | ${row.wallMs.toFixed(1)} `
    + `| ${row.cpuMs.toFixed(1)} | ${row.ratio.toFixed(2)}× | ${row.hwmMb.toFixed(0)} | ${row.warmMs.toFixed(1)} |`);
}

// **★ 如实结论 ✗ ★**：**∴ 若**所有行的后端**相同**✗ ⇒ **∴ 明说**"**本机没有两路可对比 ✓"** ✓✓
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
