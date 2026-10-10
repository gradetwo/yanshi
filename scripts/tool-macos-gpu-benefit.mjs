#!/usr/bin/env node
// **★ macOS 上的 GPU 收益实测 ✗ ★**（**第 490 轮 ✓；**用户要求「在 macOS 平台测试收益」✓）
//
// **∴ 为什么要单独一条 ✗**：**`tool-render-cost-accounts.mjs` 读 `/proc/<pid>/stat`**✗
//   ⇒ **∴ 而 macOS**没有 `/proc`** ✓
//     ⇒ **∴ 于是**：**那条工具在 macOS 上**根本量不了 CPU 账** ✓ ★**** ✓✓
//   **∴ 本脚本改用 `ps`**✗（**macOS 与 Linux 都有 ✓）
//     ⇒ **∴ 于是**：**两本账**在 macOS 上也能量** ✓ ★**** ✓✓
//
// **∴ 它量什么（**用户第 594 轮的两本账 ✓）★**：
//   **∴ ① 时间账 ✗**：**墙钟**
//   **∴ ② CPU 占用账 ✗**：**进程 CPU 时间（**`ps -o time=` ⇒ 累计 utime+stime ✓）
//   ＋ **∴ ③ CPU÷墙钟 ✗**（**并行度代理 ✓）
//     ＋ **∴ ④ 常驻内存 ✗**（**`ps -o rss=` 的**采样峰值** ✓）
//       ⇒ **∴ 两本账**都要打印**✗ ⇒ **∴ 若**只有墙钟变好**而** CPU 占用没降**✗
//         ⇒ **∴ 脚本会**明说** ✓（**∴ 不许**只报好看的那本 ✓）★**** ✓✓
//
// **∴ 它同时守的三条硬约束（**目标第 7 条 ✓）★**：
//   **∴ ①** `--gpu off` ⇒ `render_backend` **必须**恰好是 `cpu`** ✓
//   **∴ ②** `--gpu on` ⇒ **要么** `gpu`**✗ ，**要么** `cpu` ＋ **非空**的适配器说明** ✓
//     （**∴ 不许**静默降级 ✓）★**** ✓✓
//   **∴ ③** **macOS 上适配器说明必须含 `metal`**✗
//     ⇒ **∴ 若**它看起来像**软件渲染（`llvmpipe`／`Gl｜Cpu` ✓）**✗
//       ⇒ **∴ 直接判红** ✓ —— **∴ 第 318 轮**正是把软件渲染当成了 L4** ✓ ★**** ✓✓
//   ＋ **∴ ④** `render_backend = gpu` ⇒ `max_channel_delta` **必须**恰为 `0`** ✓
//     （**∴ 不许**用 0 冒充：**没比对时必须是 `null` ✓）★**** ✓✓
//
// **∴ 用法 ✗**：
//   node scripts/tool-macos-gpu-benefit.mjs --binary target/release/yanshi-serve --rounds 3
//   **∴ 在非 macOS 上开发本脚本 ✗**：`YANSHI_ALLOW_NON_MACOS=1 node …` ✓
//     （**∴ 那时**跳过「必须含 metal」那一条**✗ ，**其余照跑** ✓）
//   **∴ 自检（**证明判据有牙 ✓）**：`--self-test` ✓
//
// **∴ 退出码 ✗**：**0 ＝ 两本账都拿到了且约束都满足**✗
//   ｜**1 ＝ 约束不满足**✗｜**2 ＝ 环境／用法问题**✗
//   ｜**★ 3 ＝ 判据自身跑不了**（**不是产品问题 ✓）★** —— **∴ 按 AGENTS.md ✓**

import { execSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// ─────────────────────────── 参数 ───────────────────────────
const argv = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && i + 1 < argv.length ? argv[i + 1] : fallback;
};
const has = (name) => argv.includes(name);

const binary = opt("--binary", "target/release/yanshi-serve");
const rounds = Number(opt("--rounds", "3"));
const width = Number(opt("--width", "3840"));
const height = Number(opt("--height", "2160"));
const outPath = opt("--out", "target/macos-gpu-benefit.md");
const jsonPath = opt("--json", "target/macos-gpu-benefit.json");
const basePort = Number(opt("--port", "8811"));
const keepTemp = has("--keep-temp");

const isMac = process.platform === "darwin";
const allowNonMac = process.env.YANSHI_ALLOW_NON_MACOS === "1";

// ─────────────────────── 判据自身的前提 ───────────────────────
// **∴ 判据要能说「**我自己坏了**」✗ ★**（**AGENTS.md ✓）
if (!existsSync(binary)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到二进制 ${binary}`);
  console.error("     ⇒ 先构建：cargo build --release -p yanshi-http --bin yanshi-serve --features gpu");
  process.exit(3);
}
if (!isMac && !allowNonMac && !has("--self-test")) {
  console.error("  ✗ 本判据只在 macOS 上有意义（不是产品缺陷）。");
  console.error("     ⇒ 在 macOS 上跑：node scripts/tool-macos-gpu-benefit.mjs");
  console.error("     ⇒ 在别处开发本脚本：YANSHI_ALLOW_NON_MACOS=1 node scripts/tool-macos-gpu-benefit.mjs");
  process.exit(3);
}

// ─────────────────────── 进程测量（**用 ps ✓） ───────────────────────
/** **∴ 解析 `ps -o time=` ✗**：`[[dd-]hh:]mm:ss[.ss]` ⇒ **毫秒** ✓ */
export function parsePsTime(text) {
  const t = String(text).trim();
  if (!t) return null;
  let days = 0;
  let rest = t;
  const dash = rest.indexOf("-");
  if (dash >= 0) {
    days = Number(rest.slice(0, dash));
    rest = rest.slice(dash + 1);
  }
  const parts = rest.split(":").map((x) => Number(x));
  if (parts.some((x) => Number.isNaN(x))) return null;
  let h = 0;
  let m = 0;
  let s = 0;
  if (parts.length === 3) [h, m, s] = parts;
  else if (parts.length === 2) [m, s] = parts;
  else if (parts.length === 1) [s] = parts;
  else return null;
  return ((days * 24 + h) * 3600 + m * 60 + s) * 1000;
}

/** **∴ 时钟节拍 ✗**（**Linux 上 `/proc` 用它换算；**通常 100 ⇒ 1 拍 ＝ 10 ms ✓） */
const CLK_TCK = (() => {
  try {
    return Number(execSync("getconf CLK_TCK", { encoding: "utf8" }).trim()) || 100;
  } catch { return 100; }
})();

/** **★ 读一个进程的 CPU 时间与常驻内存 ✗ ★**
 *
 * **∴ 为什么在两平台上用不同来源 ✗**（**第 490 轮实测 ✓）**：
 *   **∴ Linux 的 `ps -o time=` **只有**整秒**精度**✗
 *     ⇒ **∴ 实测**两行都恰好是 `1000.0 ms`**✗ ⇒ **∴ 于是**：**CPU 账**完全不可判** ✓
 *       ＋ **∴ 所以**：**Linux 走 `/proc/<pid>/stat`**✗
 *         ⇒ **∴ 它**给的是**时钟节拍**（**1 拍 ＝ 10 ms ✓）★**** ✓✓
 *   **∴ macOS 没有 `/proc` ✗** ⇒ **∴ 只能**用 `ps -o time=`** ✓
 *     ＋ **∴ 而**它在那边的分辨率**是**厘秒** ⇒ **∴ 可用** ✓ ★**** ✓✓
 *
 * **∴ 返回值里的 `resMs` ✗**：**本平台的**CPU 时间分辨率**✗
 *   ⇒ **∴ 报出它**✗ ⇒ **∴ 读者**才知道这个数**能信到哪一位** ✓（**∴ 不许**假装精确 ✓）★**** ✓✓
 */
function readProc(pid) {
  if (process.platform === "linux") {
    return new Promise((resolve) => {
      try {
        const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
        // **∴ 字段 14／15（**1-based ✓）＝ utime／stime**✗
        //   ＋ **∴ 而**comm 可能含空格与括号**✗ ⇒ **∴ 从最后一个 `)` 之后切** ✓
        const tail = stat.slice(stat.lastIndexOf(")") + 1).trim().split(/\s+/);
        const utime = Number(tail[11]);
        const stime = Number(tail[12]);
        const rssPages = Number(tail[21]);
        const cpuMs = ((utime + stime) * 1000) / CLK_TCK;
        const rssKb = (rssPages * 4096) / 1024; // **∴ 页 ⇒ KB（**4 KiB 页 ✓）
        resolve({ cpuMs, rssKb, resMs: 1000 / CLK_TCK });
      } catch { resolve(null); }
    });
  }
  return new Promise((resolve) => {
    const ps = spawn("ps", ["-o", "time=", "-o", "rss=", "-p", String(pid)], { stdio: ["ignore", "pipe", "ignore"] });
    let buf = "";
    ps.stdout.on("data", (d) => { buf += d; });
    ps.on("close", () => {
      const line = buf.trim().split("\n").pop() || "";
      const m = line.trim().match(/^(\S+)\s+(\d+)$/);
      if (!m) return resolve(null);
      // **∴ macOS 的 `time` 形如 `0:00.05`**✗ ⇒ **∴ 分辨率**厘秒 ✓
      resolve({ cpuMs: parsePsTime(m[1]), rssKb: Number(m[2]), resMs: 10 });
    });
    ps.on("error", () => resolve(null));
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (xs) => {
  const a = [...xs].sort((x, y) => x - y);
  if (a.length === 0) return 0;
  const mid = Math.floor(a.length / 2);
  return a.length % 2 ? a[mid] : (a[mid - 1] + a[mid]) / 2;
};

// ─────────────────────── 断言（**可被自检直接调用 ✓） ───────────────────────
/**
 * **★ 对 `/health` 的断言 ✗ ★**（**抽成纯函数 ⇒ 可自检 ✓）
 * @returns {string[]} 失败原因（**空 ⇒ 全过 ✓）
 */
export function checkHealth({ mode, health, isMac: mac }) {
  const bad = [];
  const rb = health.render_backend;
  if (rb !== "cpu" && rb !== "gpu") bad.push(`render_backend 必须是真值之一（实测 ${rb}）`);
  if (mode === "off" && rb !== "cpu") bad.push(`--gpu off ⇒ render_backend 必须恰好是 cpu（实测 ${rb}）`);
  if (mode === "on" && rb !== "gpu") {
    const note = health.gpu_adapter_note ?? health.gpu_unavailable_reason;
    if (typeof note !== "string" || note.length === 0) {
      bad.push("--gpu on 而实际不是 gpu ⇒ 必须给出适配器说明（否则是静默降级）");
    }
  }
  // **★ 软件渲染不许冒充硬件 ✗ ★**（**第 318 轮的教训 ✓）
  const note = String(health.gpu_adapter_note ?? health.gpu_unavailable_reason ?? "");
  if (mac && health.gpu_mode !== "off") {
    if (!/metal/i.test(note)) {
      bad.push(`macOS 上适配器说明必须含 metal（实测：${note || "（空）"}）`);
    }
    if (/llvmpipe|swiftshader|Gl\|Cpu|\|Cpu\b/i.test(note)) {
      bad.push(`适配器看起来是软件渲染 ⇒ 不许当成硬件加速（实测：${note}）`);
    }
  }
  // **∴ 逐位一致：**没比对时必须是 null**✗ ，**有比对且一致才是 0** ✓
  if (rb === "gpu" && health.max_channel_delta !== 0) {
    bad.push(`render_backend = gpu ⇒ max_channel_delta 必须恰为 0（实测 ${JSON.stringify(health.max_channel_delta)}）`);
  }
  if (rb !== "gpu" && health.max_channel_delta === 0) {
    bad.push("没有走 GPU 却报 max_channel_delta = 0 ⇒ 那是**用 0 冒充**比过");
  }
  return bad;
}

// ─────────────────────── 自检（**证明判据有牙 ✓） ───────────────────────
if (has("--self-test")) {
  console.log("  ── 自检：断言**必须能红** ──");
  const cases = [
    { name: "off 却报 gpu", args: { mode: "off", health: { render_backend: "gpu", gpu_mode: "off", max_channel_delta: 0 }, isMac: false }, wantRed: true },
    { name: "on 却报 cpu 且无说明", args: { mode: "on", health: { render_backend: "cpu", gpu_mode: "on", max_channel_delta: null }, isMac: false }, wantRed: true },
    { name: "macOS 上适配器不是 metal", args: { mode: "on", health: { render_backend: "gpu", gpu_mode: "on", gpu_adapter_note: "Gl|IntegratedGpu|Intel", max_channel_delta: 0 }, isMac: true }, wantRed: true },
    { name: "macOS 上是 llvmpipe", args: { mode: "on", health: { render_backend: "gpu", gpu_mode: "on", gpu_adapter_note: "Gl|Cpu|llvmpipe", max_channel_delta: 0 }, isMac: true }, wantRed: true },
    { name: "gpu 而 delta 不是 0", args: { mode: "on", health: { render_backend: "gpu", gpu_mode: "on", gpu_adapter_note: "Vulkan|DiscreteGpu|NVIDIA", max_channel_delta: null }, isMac: false }, wantRed: true },
    { name: "没走 GPU 却报 delta = 0", args: { mode: "off", health: { render_backend: "cpu", gpu_mode: "off", max_channel_delta: 0 }, isMac: false }, wantRed: true },
    { name: "正常：cpu ＋ null", args: { mode: "off", health: { render_backend: "cpu", gpu_mode: "off", max_channel_delta: null }, isMac: false }, wantRed: false },
    { name: "正常：macOS 走 Metal", args: { mode: "on", health: { render_backend: "gpu", gpu_mode: "on", gpu_adapter_note: "Metal|IntegratedGpu|Apple M3", max_channel_delta: 0 }, isMac: true }, wantRed: false },
  ];
  let red = 0;
  let wrong = 0;
  for (const c of cases) {
    const failed = checkHealth(c.args);
    const wentRed = failed.length > 0;
    const ok = wentRed === c.wantRed;
    if (wentRed) red += 1;
    if (!ok) wrong += 1;
    console.log(`  ${ok ? "✓" : "✗"} ${c.name} ⇒ ${wentRed ? "红" : "绿"}（期望${c.wantRed ? "红" : "绿"}）`);
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) { console.error("  ✗ 自检失败 ⇒ **本判据没有牙**"); process.exit(1); }
  console.log("  ✓ 自检通过：断言既能绿也能红（**有牙 ✓）");
  process.exit(0);
}

// ─────────────────────── 起服务 ＋ 建场景 ───────────────────────
async function startServer(mode, tag) {
  const root = mkdtempSync(join(tmpdir(), `macosgpu-${tag}-`));
  const port = basePort + (mode === "on" ? 1 : 0);
  const args = [
    "--root", root,
    "--bind", `127.0.0.1:${port}`,
    "--doc", "boot",
    "--width", String(width),
    "--height", String(height),
    "--assets-dir", "assets",
    "--gpu", mode,
  ];
  const child = spawn(binary, args, { stdio: ["ignore", "pipe", "pipe"] });
  let log = "";
  child.stdout.on("data", (d) => { log += d; });
  child.stderr.on("data", (d) => { log += d; });
  const base = `http://127.0.0.1:${port}`;
  let health = null;
  for (let i = 0; i < 60; i += 1) {
    try {
      const r = await fetch(`${base}/health`);
      if (r.ok) { health = await r.json(); break; }
    } catch { /* 还没起来 */ }
    await sleep(500);
  }
  if (!health) {
    try { child.kill("SIGKILL"); } catch { /* 已退出 */ }
    return { mode, tag, base, child, health: null, log, root };
  }
  return { mode, tag, base, child, health, log, root };
}

async function buildScenario(srv) {
  const docId = `macosgpu_${srv.mode}`;
  const created = await (await fetch(`${srv.base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width, height }),
  })).json();
  const q = `doc=${docId}&token=${created.token}`;
  const call = async (tool, args) =>
    (await fetch(`${srv.base}/api/tools/${tool}?${q}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })).json();
  for (const id of ["L0", "L1", "L2", "L3"]) await call("create_layer", { layer_id: id });
  for (let i = 0; i < 4; i += 1) {
    await call("fill", {
      layer_id: `L${i}`,
      object_id: `o${i}`,
      data: {
        color: { r: 30 + i * 40, g: 120, b: 200, a: 255 },
        region: { x: i * 200, y: i * 150, width: Math.round(width * 0.6), height: Math.round(height * 0.6) },
      },
    });
  }
  return { q, call };
}

// **∴ 一次渲染：**量墙钟 ＋ CPU 时间 ＋ 常驻内存 ✓
//
// **★ 每轮必须先改一处内容 ✗ ★**（**第 47 轮已记录的坑 ✓，**本轮又踩了一次 ✓）
//   **∴ 症状 ✗**：**第二轮起**墙钟 **1.8 ms**✗（**∴ 因为**命中了**区域／below 缓存** ✓）
//     ⇒ **∴ 那样量的是**缓存**✗ ，**不是渲染** ✓
//       ＋ **∴ 于是**：**两本账**全是噪声** ✓ ★**** ✓✓
//   **∴ 做法 ✗**：**每轮**挪动一个填充的 region**✗ ⇒ **∴ 内容变 ⇒ **∴ 缓存失效 ✓ ★**** ✓✓
async function measureRound(srv, scenario, { round, warm }) {
  const { q, call } = scenario;
  await call("fill", {
    layer_id: "L0",
    object_id: "o_churn",
    data: {
      color: { r: 200, g: 30, b: 60, a: 255 },
      region: { x: round * 3, y: round * 2, width: Math.round(width * 0.3), height: Math.round(height * 0.4) },
    },
  });
  const before = await readProc(srv.child.pid);
  const t0 = process.hrtime.bigint();
  const res = await fetch(`${srv.base}/api/tools/render_region?${q}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: [0, 0, width, height], include_image: false }),
  });
  const ok = res.ok;
  await res.arrayBuffer().catch(() => null);
  const t1 = process.hrtime.bigint();
  const after = await readProc(srv.child.pid);
  const wallMs = Number(t1 - t0) / 1e6;
  const cpuMs = before && after ? after.cpuMs - before.cpuMs : null;
  const resMs = after ? after.resMs : null;
  return {
    round, warm, ok, wallMs, cpuMs, resMs,
    rssKb: after ? after.rssKb : null,
    ratio: cpuMs != null && wallMs > 0 ? cpuMs / wallMs : null,
  };
}

// ─────────────────────── 主流程 ───────────────────────
const servers = [];
let exitCode = 0;
try {
  console.log("");
  console.log("  ★ macOS GPU 收益实测（两本账）★");
  console.log(`  平台 = ${process.platform}｜二进制 = ${binary}｜场景 = ${width}×${height}×4 层｜轮数 = ${rounds}`);
  if (!isMac) console.log("  ⚠️ 不在 macOS 上（YANSHI_ALLOW_NON_MACOS=1）⇒ 跳过「适配器必须含 metal」那一条");

  for (const mode of ["off", "on"]) {
    servers.push(await startServer(mode, mode));
  }
  for (const s of servers) {
    if (!s.health) {
      console.error(`  ✗ --gpu ${s.mode}：服务没起来（60 次重试都失败）`);
      console.error(`     日志尾部：${s.log.slice(-400)}`);
      process.exit(2);
    }
  }

  // **∴ 启动时先看一眼（**只作参考 ✓）★**
  for (const s of servers) {
    console.log(`  · --gpu ${s.mode}（启动时）⇒ render_backend=${s.health.render_backend}｜gpu_mode=${s.health.gpu_mode}`);
    console.log(`      适配器：${String(s.health.gpu_adapter_note ?? s.health.gpu_unavailable_reason ?? "（空）").slice(0, 110)}`);
  }
  for (const s of servers) s.scenario = await buildScenario(s);

  // **∴ 测量：**第一轮丢弃（**冷页缓存 ✓）★**
  const rows = [];
  for (const s of servers) {
    const samples = [];
    for (let i = 0; i < rounds; i += 1) {
      samples.push(await measureRound(s, s.scenario, { round: i + 1, warm: i === 0 }));
    }
    const usable = samples.length > 1 ? samples.slice(1) : samples;
    // **★ 必须在渲染之后重读 `/health` ✗ ★**（**第 460／462 轮的同一个错 ✓）：
    //   **∴ 渲染前** `render_backend` 还是初值 `cpu`**✗
    //     ⇒ **∴ 于是**：**GPU 那行会**显示成 `cpu`** ✓
    //       ＋ **∴ 而那**会让本脚本**误判** ✓ ★**** ✓✓
    try {
      s.post = await (await fetch(`${s.base}/health`)).json();
    } catch {
      s.post = s.health;
    }
    rows.push({
      label: s.mode === "off" ? "CPU" : "GPU",
      mode: s.mode,
      backend: s.post.render_backend,
      adapter: String(s.post.gpu_adapter_note ?? s.post.gpu_unavailable_reason ?? ""),
      delta: s.post.max_channel_delta,
      wall: median(usable.map((x) => x.wallMs)),
      cpu: median(usable.filter((x) => x.cpuMs != null).map((x) => x.cpuMs)),
      ratio: median(usable.filter((x) => x.ratio != null).map((x) => x.ratio)),
      rssMb: Math.max(...samples.filter((x) => x.rssKb != null).map((x) => x.rssKb)) / 1024,
      warmWall: samples[0].wallMs,
      resMs: samples.map((x) => x.resMs).find((x) => x != null) ?? null,
      ok: samples.every((x) => x.ok),
    });
  }

  // **∴ 两张表都要出 ✗**（**∴ 不许**只报好看的那本 ✓）
  // **∴ 诚实性断言（**渲染后 ✓）★**
  const badAll = [];
  for (const s of servers) {
    const bad = checkHealth({ mode: s.mode, health: s.post, isMac });
    for (const b of bad) badAll.push(`--gpu ${s.mode}：${b}`);
    if (s.mode === "on" && s.post.render_backend !== "gpu") {
      console.log(`  ⚠️ --gpu on 渲染后仍是 cpu ⇒ 原因：${String(s.post.gpu_adapter_note ?? "（空）").slice(0, 100)}`);
    }
  }

  const [cpu, gpu] = rows;
  const pct = (a, b) => (a > 0 ? ((b - a) / a) * 100 : NaN);
  const fmt = (x, d = 1) => (Number.isFinite(x) ? x.toFixed(d) : "—");

  const lines = [];
  lines.push("## macOS GPU 收益实测（两本账）");
  lines.push("");
  lines.push(`- 平台：\`${process.platform}\`｜二进制：\`${binary}\``);
  lines.push(`- 场景：${width}×${height}，4 层填充｜轮数：${rounds}（第一轮为预热，已剔除）`);
  lines.push(`- 适配器（GPU 行）：\`${gpu.adapter || "（空）"}\``);
  lines.push("");
  lines.push("| 配置 | 后端 | ① 墙钟 ms | ② 进程 CPU ms | CPU÷墙钟 | 采样峰值 RSS MB | 预热墙钟 ms |");
  lines.push("|---|---|---|---|---|---|---|");
  for (const r of rows) {
    lines.push(`| ${r.label}（--gpu ${r.mode}） | ${r.backend} | ${fmt(r.wall)} | ${fmt(r.cpu)} | ${fmt(r.ratio, 2)}× | ${fmt(r.rssMb, 0)} | ${fmt(r.warmWall)} |`);
  }
  lines.push("");
  lines.push("### 两本账（**逐项对照**）");
  lines.push("");
  lines.push(`- **① 时间账**：CPU ${fmt(cpu.wall)} ms ⇒ GPU ${fmt(gpu.wall)} ms，**${fmt(-pct(cpu.wall, gpu.wall))}%**（正数＝GPU 更快）`);
  lines.push(`- **② CPU 占用账**：CPU ${fmt(cpu.cpu)} ms ⇒ GPU ${fmt(gpu.cpu)} ms，**${fmt(-pct(cpu.cpu, gpu.cpu))}%**（正数＝GPU 更省）`);
  lines.push(`- **③ 峰值 RSS**：${fmt(cpu.rssMb, 0)} ⇒ ${fmt(gpu.rssMb, 0)} MB（**采样峰值**，非内核 HWM）`);
  // **★ 必须声明分辨率 ✗ ★**（**否则读者会以为这个数**精确到毫秒** ✓）
  const resMs = Math.max(...rows.map((r) => r.resMs ?? 0)) || null;
  lines.push(`- **CPU 时间分辨率**：${resMs == null ? "未知" : `±${resMs} ms（本平台）`}`);
  if (resMs != null && cpu.cpu > 0 && cpu.cpu < resMs * 10) {
    lines.push(`  - ⚠️ **CPU 账的绝对量偏小**（${fmt(cpu.cpu)} ms ＜ 10×分辨率）⇒ **这一列只宜看趋势，不宜看小数位**。`);
  }
  lines.push(`- **④ 逐位一致**：GPU 行 \`max_channel_delta = ${JSON.stringify(gpu.delta)}\``);
  lines.push("");
  const timeBetter = gpu.wall < cpu.wall;
  const cpuBetter = gpu.cpu <= cpu.cpu;
  lines.push("### 结论（**如实**）");
  lines.push("");
  if (timeBetter && cpuBetter) lines.push("- ✅ 两本账都变好。");
  else if (!timeBetter && !cpuBetter) lines.push("- ❌ 两本账都变差 ⇒ **不要默认开**。（如实报告，不挑好看的报）");
  else if (!timeBetter) lines.push(`- ⚠️ **时间账**变差（GPU 慢 ${fmt(pct(cpu.wall, gpu.wall))}%），**而 CPU 占用账**${cpuBetter ? "变好" : "也变差"} ⇒ **不能说「GPU 更快」**。`);
  else lines.push(`- ⚠️ **时间账**变好，**而 CPU 占用账**变差 ⇒ **按用户第 594 轮的要求必须明说**。`);

  const text = lines.join("\n");
  console.log("");
  console.log(text.split("\n").map((l) => `  ${l}`).join("\n"));

  if (badAll.length > 0) {
    console.log("");
    console.error(`  ✗ 后端诚实性有 ${badAll.length} 条不达标：`);
    for (const b of badAll) console.error(`     - ${b}`);
    exitCode = 1;
  } else {
    console.log("");
    console.log("  ✓ 后端诚实性：全部达标（off⇒cpu｜on⇒gpu 或给出非空原因｜gpu⇒delta=0）");
  }

  try {
    writeFileSync(outPath, text + "\n");
    writeFileSync(jsonPath, JSON.stringify({ platform: process.platform, width, height, rounds, rows, failures: badAll }, null, 2));
    console.log(`  · 报告已写：${outPath}｜${jsonPath}`);
  } catch (e) {
    console.error(`  ⚠️ 写报告失败（不影响结论）：${String(e).slice(0, 120)}`);
  }
} catch (error) {
  console.error(`  ✗ 本跑没有结论：${String(error && error.message || error).slice(0, 200)}`);
  exitCode = 2;
} finally {
  for (const s of servers) {
    try { s.child.kill("SIGKILL"); } catch { /* 已退出 */ }
    if (!keepTemp && s.root) {
      try { rmSync(s.root, { recursive: true, force: true }); } catch { /* 忽略 */ }
    }
  }
}
process.exit(exitCode);
