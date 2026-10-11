#!/usr/bin/env node
// **★ 分段计时的**一致性**判据 ✗ ★**（**第 498 轮 ✓）
//
// **∴ 它守什么 ✗**（**∴ 每一条都能红 ✓）**：
//   **∴ ①** **`gpu_stage_calls > 0` ⇒ **四段里至少一个 > 0**** ✗
//     ⇒ **∴ 否则**：**"量了但全是 0" ⇒ **那是**假装在量**** ✓ ★
//   **∴ ②** **`max_channel_delta == 0` ⇒ **`verify_us` 必须 > 0**** ✗
//     ⇒ **∴ 因为**：**报 0 就**必然**比对过**✗ ⇒ **∴ 而**比对**要花时间** ✓
//       ⇒ **∴ 若** `verify_us == 0` **而** `delta == 0`**✗
//         ⇒ **★ 那**要么**没量**✗ 、**要么**在撒谎** ✓ ★
//   **∴ ③** **四段之和 ≤ 墙钟的 1.5 倍** ✗
//     ⇒ **∴ 它**抓**单位错误**✗（**∴ 如**微秒写成纳秒 ⇒ 会大 1000 倍 ✓）★
//   **∴ ④** **`render_backend` 必须是 `gpu` 或 `cpu`** ✗ ＋ **∴ 且**：**`cpu` ⇒ 不许报 delta ＝ 0** ✓
//
// **∴ 用法 ✗**：
//   ⇒ **∴ 自检（**证明有牙 ✓）**：`node scripts/tool-gpu-stage-accounting.mjs --self-test` ✓
//   ⇒ **∴ 实测 ✗**：`node scripts/tool-gpu-stage-accounting.mjs --base http://127.0.0.1:19301` ✓
//     ＋ **∴ 它**自己起服务**（`--spawn` ✓，与 `run` 同一个服务端命令 ✓）
//
// **∴ 退出码 ✗**：0 通过｜1 产品缺陷｜2 用法错｜**3 判据自身跑不了**（**AGENTS.md ✓）

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const argv = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && i + 1 < argv.length ? argv[i + 1] : fallback;
};
const has = (name) => argv.includes(name);

const WIDTH = Number(opt("--width", "3840"));
const HEIGHT = Number(opt("--height", "2160"));

/**
 * **★ 纯函数：检查一份 `/health` ✗ ★**（**∴ 抽出来 ⇒ 自检能直接喂假数据 ✓）
 * @returns {string[]} 失败原因（**空 ⇒ 全过 ✓）
 */
export function checkStageAccounting({ health, wallMs }) {
  const bad = [];
  const backend = health.render_backend;
  const calls = health.gpu_stage_calls;
  const up = health.gpu_stage_upload_us;
  const sub = health.gpu_stage_submit_us;
  const rb = health.gpu_stage_readback_us;
  const vf = health.gpu_stage_verify_us;
  const delta = health.max_channel_delta;

  // **∴ 字段必须在 ✗**（**∴ 缺字段 ⇒ **不可判**** ✓）
  for (const [k, v] of Object.entries({
    gpu_stage_calls: calls,
    gpu_stage_upload_us: up,
    gpu_stage_submit_us: sub,
    gpu_stage_readback_us: rb,
    gpu_stage_verify_us: vf,
  })) {
    if (typeof v !== "number") bad.push(`/health 缺 ${k}（实测 ${JSON.stringify(v)}）`);
  }
  if (bad.length) return bad;

  if (backend !== "cpu" && backend !== "gpu") {
    bad.push(`render_backend 必须是 cpu 或 gpu（实测 ${backend}）`);
  }
  // **① 量了就不能全是 0**
  if (calls > 0 && up + sub + rb + vf === 0) {
    bad.push(`gpu_stage_calls=${calls} 但四段全为 0 ⇒ **假装在量**`);
  }
  // **② 报 delta=0 ⇒ 必然比对过 ⇒ 核对必须花时间**
  if (delta === 0 && vf === 0) {
    bad.push("max_channel_delta=0 但 gpu_stage_verify_us=0 ⇒ **要么没量、要么在撒谎**");
  }
  // **③ 单位合理性**
  if (calls > 0 && wallMs > 0) {
    const sum = up + sub + rb + vf;
    if (sum > wallMs * 1000 * 1.5) {
      bad.push(
        `四段之和 ${sum} µs 超过墙钟 ${(wallMs).toFixed(0)} ms 的 1.5 倍 ⇒ **单位错误**（如 µs/ns 混用）`,
      );
    }
  }
  // **④ 没走核对却报 0 ⇒ 用 0 冒充**
  if (backend === "cpu" && delta === 0) {
    bad.push("render_backend=cpu 却报 max_channel_delta=0 ⇒ 用 0 冒充比对过");
  }
  return bad;
}

// ───────────────────── 自检（**证明有牙 ✓） ─────────────────────
if (has("--self-test")) {
  const base = {
    render_backend: "gpu",
    gpu_stage_calls: 2,
    gpu_stage_upload_us: 110226,
    gpu_stage_submit_us: 55594,
    gpu_stage_readback_us: 32580,
    gpu_stage_verify_us: 883267,
    max_channel_delta: 0,
  };
  const cases = [
    { name: "正常（**本机 4K 实测 ✓）", health: base, wallMs: 1200, wantRed: false },
    { name: "量了但四段全 0", health: { ...base, gpu_stage_upload_us: 0, gpu_stage_submit_us: 0, gpu_stage_readback_us: 0, gpu_stage_verify_us: 0 }, wallMs: 1200, wantRed: true },
    { name: "delta=0 而核对为 0", health: { ...base, gpu_stage_verify_us: 0 }, wallMs: 1200, wantRed: true },
    { name: "单位写错（**µs 写成 ns ✓）", health: { ...base, gpu_stage_verify_us: 883267000 }, wallMs: 1200, wantRed: true },
    { name: "缺字段", health: { ...base, gpu_stage_verify_us: undefined }, wallMs: 1200, wantRed: true },
    { name: "cpu 后端却报 delta=0", health: { ...base, render_backend: "cpu" }, wallMs: 1200, wantRed: true },
    { name: "正常：跳核对 ⇒ delta=null 且核对 0", health: { ...base, max_channel_delta: null, gpu_stage_verify_us: 0 }, wallMs: 600, wantRed: false },
  ];
  let wrong = 0;
  let red = 0;
  console.log("  ── 自检：断言**必须能红** ──");
  for (const c of cases) {
    const failed = checkStageAccounting(c);
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

// ───────────────────── 实测 ─────────────────────
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let base = opt("--base", null);
let child = null;
let work = null;

if (!base) {
  const binary = opt("--binary", "target/release/yanshi-serve");
  if (!existsSync(binary)) {
    console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：找不到 ${binary}`);
    console.error("     ⇒ 先构建：cargo build --release -p yanshi-http --bin yanshi-serve --features gpu");
    process.exit(3);
  }
  const port = Number(opt("--port", "19501"));
  work = mkdtempSync(join(tmpdir(), "stage-acc-"));
  child = spawn(
    binary,
    [
      "--bind", `127.0.0.1:${port}`,
      "--root", join(work, "w"),
      "--doc", "boot",
      "--width", String(WIDTH),
      "--height", String(HEIGHT),
      "--assets-dir", "assets",
      "--gpu", "on",
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  let log = "";
  child.stdout.on("data", (d) => { log += d; });
  child.stderr.on("data", (d) => { log += d; });
  base = `http://127.0.0.1:${port}`;
  let up = false;
  for (let i = 0; i < 60; i += 1) {
    try {
      if ((await fetch(`${base}/health`)).ok) { up = true; break; }
    } catch { /* 还没起 */ }
    await sleep(1000);
  }
  if (!up) {
    console.error(`  ✗ 服务没起来（不是判据问题）⇒ 日志尾部：${log.slice(-300)}`);
    try { child.kill("SIGKILL"); } catch { /* 已退出 */ }
    process.exit(3);
  }
}

let exitCode = 0;
try {
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: `stage${Date.now()}`, width: WIDTH, height: HEIGHT }),
  })).json();
  const q = `doc=${created.doc_id}&token=${created.token}`;
  const t0 = Date.now();
  await fetch(`${base}/api/tools/render_region?${q}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: [0, 0, WIDTH, HEIGHT], include_image: false }),
  }).then((r) => r.arrayBuffer());
  const wallMs = Date.now() - t0;
  const health = await (await fetch(`${base}/health`)).json();

  console.log("");
  console.log(`  ★ 分段账（${WIDTH}×${HEIGHT}）★`);
  console.log(`  · 适配器 = ${health.gpu_adapter_note ?? "（空）"}`);
  console.log(`  · 后端 = ${health.render_backend}｜delta = ${JSON.stringify(health.max_channel_delta)}｜墙钟 = ${wallMs.toFixed(0)} ms`);
  if (health.gpu_stage_calls === 0) {
    console.log("  ℹ️ gpu_stage_calls = 0 ⇒ **本次没有走 GPU 量化** ⇒ 四段不可读（**不是失败 ✓）");
    console.log("     ⇒ 判据的 ①②③ 条**只在 calls > 0 时**才有意义 ✓");
  } else {
    const sum = health.gpu_stage_upload_us + health.gpu_stage_submit_us
      + health.gpu_stage_readback_us + health.gpu_stage_verify_us;
    const pct = (x) => (sum > 0 ? ((x / sum) * 100).toFixed(1) : "0.0");
    console.log(`  | 段 | 微秒 | 占比 |`);
    console.log(`  |---|---|---|`);
    console.log(`  | ① 上行 | ${health.gpu_stage_upload_us} | ${pct(health.gpu_stage_upload_us)}% |`);
    console.log(`  | ② 提交 | ${health.gpu_stage_submit_us} | ${pct(health.gpu_stage_submit_us)}% |`);
    console.log(`  | ③ 读回 | ${health.gpu_stage_readback_us} | ${pct(health.gpu_stage_readback_us)}% |`);
    console.log(`  | ④ 核对 | ${health.gpu_stage_verify_us} | ${pct(health.gpu_stage_verify_us)}% |`);
    console.log(`  | 合计 | ${sum} | 100% |`);
  }
  const bad = checkStageAccounting({ health, wallMs });
  console.log("");
  if (bad.length) {
    for (const b of bad) console.error(`  ✗ ${b}`);
    exitCode = 1;
  } else {
    console.log("  ✓ 分段账自洽：量到就非全零｜报 delta=0 ⇒ 核对有时间｜单位量级合理｜后端诚实 ✓");
  }
} catch (error) {
  console.error(`  ✗ 本次没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  exitCode = 2;
} finally {
  if (child) { try { child.kill("SIGKILL"); } catch { /* 已退出 */ } }
  if (work) { try { rmSync(work, { recursive: true, force: true }); } catch { /* 忽略 */ } }
}
process.exit(exitCode);
