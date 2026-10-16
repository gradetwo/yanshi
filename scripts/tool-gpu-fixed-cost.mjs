#!/usr/bin/env node
// **★ GPU 路的**固定开销**判据 ✗ ★**（**第 512 轮 ✓）
//
// **★★★ 它守的是一个**已修的真性能 bug**** ★★★**：
//   **∴ 症状 ✗**：**`--gpu on` 时**极小渲染**（8 100 px）要 **34.9 ms**✗
//     ＋ **∴ 而**纯 CPU 只要 **3.2 ms** ✓（**∴ 差 **10.9 倍**** ✓）
//       ＋ **∴ 而**那时 `render_backend` **还是 `cpu`** ⇒ **∴ 白付 ✓** ★
//   **∴ 真因 ✗**：**`buffer.rs` 每次渲染都调 `Quantizer::new(lut)`** ✓
//     ⇒ **∴ 而**它内部建 `wgpu::Instance` ＋ `request_adapter`
//       （**`yanshi-gpu/src/lib.rs:271-273` ✓）⇒ **∴ 实测 ≈ **32 ms**** ✓
//     ＋ **∴ 更糟 ✗**：**规模阈值**必须先拿 `device_class`**
//       ⇒ **∴ 而**那**必须先建 quantizer** ✓
//         ⇒ **∴ 所以**：**小图**即使最终走 CPU，也白付 32 ms** ✓ ★**** ✓✓
//   **∴ 修法 ✗**：**把 `Quantizer` 缓存成**进程级单例**（**`OnceLock<Mutex<Option<…>>>` ✓）
//     ⇒ **∴ 而** LUT 是全局常量 ⇒ **∴ 复用安全** ✓ ★
//
// **∴ 本判据做什么 ✗**：
//   ⇒ **∴ 起两个服务 ✗**：**`--gpu off`（**纯 CPU ✓）与 `--gpu on`（**纯 GPU 路 ✓）
//     ＋ **∴ 都渲染一个**极小的区域**（**默认 90×90 ＝ 8 100 px ✓）
//       ⇒ **∴ 断言 ✗**：**GPU 路的中位墙钟**不得超过**纯 CPU 的 `max(2×, +5 ms)`** ✓
//         ⇒ **∴ 即**：**不许再有**与尺寸无关的固定开销** ✓ ★
//
// **∴ 为什么用**比值 ＋ 绝对余量**✗**：**∴ 小尺寸抖动相对大**✗
//   ⇒ **∴ 只用比值**会**误报** ✓ ⇒ **∴ 加 5 ms 余量 ⇒ **∴ 判据只在**真有大固定开销**时红** ✓
//
// **∴ 用法 ✗**：`node scripts/tool-gpu-fixed-cost.mjs`｜`--self-test`
// **∴ 退出码 ✗**：0 通过｜1 产品缺陷（**固定开销回来了 ✓）｜2 用法错
//   ｜**★ 3 判据自身跑不了**（**如本机没有 GPU 路 ✓）★

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const argv = process.argv.slice(2);
const opt = (n, d) => {
  const i = argv.indexOf(n);
  return i >= 0 && i + 1 < argv.length ? argv[i + 1] : d;
};

const SIDE = Number(opt("--side", "90"));
const SAMPLES = Number(opt("--samples", "5"));
const CPU_BASE = Number(opt("--port-cpu", "19701"));
const GPU_BASE = Number(opt("--port-gpu", "19702"));

/** **∴ 纯函数：判断固定开销是否回来了 ✗**（**∴ 自检能直接喂假数据 ✓）★ */
export function checkFixedCost({ cpuMs, gpuMs, slackMs = 5, ratio = 2 }) {
  const bad = [];
  const limit = Math.max(cpuMs * ratio, cpuMs + slackMs);
  if (gpuMs > limit) {
    bad.push(
      `GPU 路中位 ${gpuMs.toFixed(1)} ms 超过上限 ${limit.toFixed(1)} ms` +
        `（纯 CPU ${cpuMs.toFixed(1)} ms）⇒ **与尺寸无关的固定开销回来了**`,
    );
  }
  return bad;
}

if (argv.includes("--self-test")) {
  const cases = [
    { name: "修好后（3.2 vs 3.5）", args: { cpuMs: 3.5, gpuMs: 3.2 }, wantRed: false },
    { name: "★ 旧 bug（3.2 vs 34.9）", args: { cpuMs: 3.2, gpuMs: 34.9 }, wantRed: true },
    { name: "轻微偏高但在余量内（3.2 vs 7.9）", args: { cpuMs: 3.2, gpuMs: 7.9 }, wantRed: false },
    { name: "刚好超余量（3.2 vs 8.3）", args: { cpuMs: 3.2, gpuMs: 8.3 }, wantRed: true },
    { name: "大尺寸上 GPU 快（140 vs 90）", args: { cpuMs: 140, gpuMs: 90 }, wantRed: false },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：断言**必须能红** ──");
  for (const c of cases) {
    const failed = checkFixedCost(c.args);
    const wentRed = failed.length > 0;
    if (wentRed) red += 1;
    const ok = wentRed === c.wantRed;
    if (!ok) wrong += 1;
    console.log(`  ${ok ? "✓" : "✗"} ${c.name} ⇒ ${wentRed ? "红" : "绿"}（期望${c.wantRed ? "红" : "绿"}）`);
  }
  console.log("");
  console.log(`  ⇒ 自检：${cases.length} 例｜判红 ${red}｜不符合期望 ${wrong}`);
  if (wrong > 0) {
    console.error("  ✗ 自检失败 ⇒ **本判据没有牙**");
    process.exit(1);
  }
  console.log("  ✓ 自检通过：断言既能绿也能红（**有牙 ✓）");
  process.exit(0);
}

const binary = opt("--binary", "target/release/yanshi-serve");
if (!existsSync(binary)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${binary}`);
  process.exit(3);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function start(mode, port) {
  const work = trackTemp(mkdtempSync(join(tmpdir(), `fixcost-${mode}-`)));
  const child = spawn(
    binary,
    ["--bind", `127.0.0.1:${port}`, "--root", join(work, "w"), "--doc", "boot",
     "--width", "512", "--height", "512", "--assets-dir", "assets", "--gpu", mode],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  // **★ 必须收尾 ✗ ★**（2026-10-16 ✓）：它起的是**长期服务** ⇒ 判据退出后不杀它 ⇒
  //   进程一直活着 ⇒ **CI 的 step 可能一直等 ⇒ 分片永不结束** ✗（"11 绿 0 红却到不了终态"同形 ✓）。
  process.on("exit", () => { try { child.kill(); } catch { /* 已退出 */ } });
  let log = "";
  child.stdout.on("data", (d) => { log += d; });
  child.stderr.on("data", (d) => { log += d; });
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 60; i += 1) {
    try {
      if ((await fetch(`${base}/health`)).ok) return { base, child, work, log: () => log };
    } catch { /* 还没有 */ }
    await sleep(500);
  }
  return { base, child, work, log: () => log, failed: true };
}

async function measure(base) {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "fc", width: 512, height: 512 }),
  })).json();
  const q = `doc=fc&token=${created.token}`;
  const call = async (tool, args) =>
    (await fetch(`${base}/api/tools/${tool}?${q}`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })).json();
  await call("create_layer", { layer_id: "L0" });
  // **∴ 暖机一次**✗：**缓存类修复**第一发总是贵的** ✓（**∴ 判据要测**稳态** ✓）
  await call("render_region", { region: [0, 0, SIDE, SIDE], include_image: false });
  const walls = [];
  for (let k = 0; k < SAMPLES; k += 1) {
    // **∴ 每轮改内容**⇒ **否则量到的是缓存** ✓（**第 47 轮的教训 ✓）
    await call("fill", {
      layer_id: "L0", object_id: "churn",
      data: { color: { r: 40 + k * 30, g: 120, b: 200, a: 255 },
              region: { x: k * 3, y: k * 2, width: SIDE, height: SIDE } },
    });
    const t0 = performance.now();
    await call("render_region", { region: [0, 0, SIDE, SIDE], include_image: false });
    walls.push(performance.now() - t0);
  }
  const health = await (await fetch(`${base}/health`)).json();
  walls.sort((a, b) => a - b);
  return { median: walls[Math.floor(walls.length / 2)], walls, adapter: health.gpu_adapter_note };
}

const servers = [];
let code = 0;
try {
  const cpu = await start("off", CPU_BASE);
  const gpu = await start("on", GPU_BASE);
  servers.push(cpu, gpu);
  if (cpu.failed || gpu.failed) {
    console.error(`  ✗ 服务没起来（**不是判据问题 ✓）⇒ ${(cpu.failed ? cpu : gpu).log().slice(-200)}`);
    process.exit(3);
  }
  const c = await measure(cpu.base);
  const g = await measure(gpu.base);
  console.log("");
  console.log(`  ★ 极小渲染（${SIDE}×${SIDE} ＝ ${SIDE * SIDE} px）★`);
  console.log(`  · 纯 CPU ：中位 ${c.median.toFixed(1)} ms（原值 ${c.walls.map((x) => x.toFixed(1)).join("／")}）`);
  console.log(`  · --gpu on：中位 ${g.median.toFixed(1)} ms（原值 ${g.walls.map((x) => x.toFixed(1)).join("／")}）`);
  console.log(`  · 适配器 = ${g.adapter ?? "（空）"}`);
  if (g.adapter && /llvmpipe/i.test(g.adapter)) {
    console.log("  ℹ️ 适配器是 llvmpipe（**软件光栅器 ✓）⇒ **本判据测的是「没有硬件」的路径** ✓");
  }
  const bad = checkFixedCost({ cpuMs: c.median, gpuMs: g.median });
  console.log("");
  if (bad.length) {
    for (const b of bad) console.error(`  ✗ ${b}`);
    code = 1;
  } else {
    console.log("  ✓ `--gpu on` 没有与尺寸无关的固定开销（**缓存生效 ✓）");
  }
} catch (error) {
  console.error(`  ✗ 本次没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  code = 2;
} finally {
  for (const s of servers) {
    if (!s) continue;
    try { s.child.kill("SIGKILL"); } catch { /* 已退出 */ }
    try { rmSync(s.work, { recursive: true, force: true }); } catch { /* 忽略 */ }
  }
}
process.exit(code);
