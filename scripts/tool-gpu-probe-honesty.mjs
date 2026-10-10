#!/usr/bin/env node
// **★ GPU 的"不可用原因"必须**探测得来**，不许硬编码** ✗ ★**（第 257 轮 ✓；**目标第 7 条 ✓）。
//
// **∴ 为什么 ✗**：**实测**（**第 256 轮 ✓）**✗**：
//   **∴ 本机有**渲染设备**✗**：
//     `/dev/dri/card1` ＋ **`/dev/dri/renderD128`** ✓
//   **∴ 而**服务端**硬编码**✗**（`server.rs:816–817` ✓）：
//     `"render_backend": "cpu"`
//     `"gpu_unavailable_reason": "host_has_no_gpu"` ✓
//     ⇒ **★ 所以**：**那个理由是**一个假设**✗，**不是**探测结果** ✓ ★**** ✓✓
//   **⇒ ∴ 而**那**正是目标第 7 条**禁止的**：
//     **"**`render_backend` 必须报**实际**后端**"** ✓
//     ＋ **"**不许静默降级**"** ✓（**∴ 硬编码的"没有"**与**硬编码的"有"**一样是撒谎 ✓）** ✓✓
//
// **∴ 本判据 ✗**：**分两级**✗
//   **∴ ① 环境级 ✗**：**读 `/dev/dri/`**✗ ⇒ **∴ 若**有 `renderD*`**✗
//     ⇒ **∴ 断言**：**`gpu_unavailable_reason` **不得**是 `host_has_no_gpu`** ✓
//   **∴ ② 一致性 ✗**：**`render_backend` **必须**与 `gpu_mode` 及原因**自洽*** ✓
//     ⇒ **∴ 如** `--gpu off` ⇒ **∴ `render_backend` 必须 `cpu`** ✓
//       ＋ **∴ 而** `--gpu on` **而** `render_backend=cpu`**✗
//         ⇒ **∴ 必须**有**非空原因** ✓（**∴ 不许**沉默地降级 ✓）** ✓✓
//
// **∴ 变异点 ✗**：**把原因写回硬编码** ⇒ **∴ ①** 必红** ✓
//
// 用法：node scripts/tool-gpu-probe-honesty.mjs

import { existsSync, readdirSync, mkdtempSync } from "node:fs";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};

// **∴ ① 环境 ✗**：本机有没有渲染设备 ✓
let hasRenderNode = false;
try {
  hasRenderNode = readdirSync("/dev/dri").some((n) => n.startsWith("renderD"));
} catch { hasRenderNode = false; }
console.log(`  环境：/dev/dri 里有渲染设备 ＝ ${hasRenderNode ? "是 ✓" : "否"}`);

const ROOT = mkdtempSync(join(tmpdir(), "gpuprobe-"));
const BIN = ["target/release/yanshi-serve", "target/debug/yanshi-serve"]
  .find((p) => existsSync(p));
if (!BIN) { console.error("  ✗ 找不到 yanshi-serve ⇒ **∴ 本跑没有结论**"); process.exit(2); }

const readHealth = async (args) => {
  const port = 19900 + Math.floor(Math.random() * 80);
  const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`, ...args], { stdio: "ignore" });
  try {
    for (let i = 0; i < 80; i += 1) {
      try { if ((await fetch(`http://127.0.0.1:${port}/health`)).ok) break; } catch { /* 未起 */ }
      await sleep(250);
    }
    const h = await (await fetch(`http://127.0.0.1:${port}/health`)).json();
    return {
      backend: h.render_backend,
      mode: h.gpu_mode,
      reason: h.gpu_unavailable_reason,
      delta: h.max_channel_delta,
      note: h.max_channel_delta_note,
    };
  } finally { proc.kill("SIGKILL"); await sleep(200); }
};

const auto = await readHealth([]);
console.log(`  --gpu auto ⇒ render_backend=${auto.backend}｜gpu_mode=${auto.mode}｜reason=${auto.reason}`);

// **∴ ① ✗**：**有渲染设备时**，不许说 `host_has_no_gpu`** ✓
if (hasRenderNode) {
  check(auto.reason !== "host_has_no_gpu",
    "本机有 /dev/dri/renderD* ⇒ `gpu_unavailable_reason` 不得是 `host_has_no_gpu`",
    `实测 reason=${auto.reason}`);
}

// **∴ ② 一致性 ✗**：**`--gpu off` ⇒ 必须 `cpu`** ✓
const off = await readHealth(["--gpu", "off"]);
console.log(`  --gpu off ⇒ render_backend=${off.backend}｜gpu_mode=${off.mode}｜reason=${off.reason}`);
check(off.backend === "cpu", "`--gpu off` ⇒ `render_backend` 必须是 cpu", `实测 ${off.backend}`);

// **∴ ③ 不许沉默降级 ✗**：`--gpu on` 而**实际 `cpu`** ⇒ **必须有非空原因** ✓
const on = await readHealth(["--gpu", "on"]);
console.log(`  --gpu on ⇒ render_backend=${on.backend}｜gpu_mode=${on.mode}｜reason=${on.reason}`);
if (on.backend === "cpu") {
  check(typeof on.reason === "string" && on.reason.length > 0,
    "`--gpu on` 而实际 cpu ⇒ 必须有**非空**的 `gpu_unavailable_reason`（不许沉默降级）",
    `实测 reason=${on.reason}`);
}

// **★ ④ `render_backend` 必须与**真实渲染**一致 ✗ ★**（第 259 轮 ✓；**目标第 7 条 ✓）。
//
// **∴ 为什么补这一条 ✗**：**第 258 轮我犯的错**（**如实 ✓）**✗**：
//   **∴ 我**把 `render_backend` 也交给探测**✗ ⇒ **∴ 于是**它**报了 `gpu`** ✓
//     ⇒ **∴ 而**渲染**实际仍然**是纯 CPU** ✓
//       ⇒ **★ 所以**：**我**把「**假的没有**」换成了「**假的有**」** ✓
//   **∴ 而**当时本判据**仍然绿** ✗ ⇒ **∴ 因为**它**只查了**原因字符串与自洽性** ✓
//     ⇒ **★ 所以**：**断言范围**必须覆盖**每一个会撒谎的字段** ✓ ★**** ✓✓
//
// **∴ 本断言 ✗**：**只要服务端**还没有** GPU 渲染路径**✗
//   ⇒ **∴ `render_backend` **必须恒为 `cpu`** ✓
//     **∴ 变异点**：**把它写成 `gpu`** ⇒ **∴ 必红** ✓
//       ＋ **∴ 而**将来**真的接了 GPU 后端**✗
//         ⇒ **∴ 那时**才**放宽它**✗ ⇒ **∴ 并**改成「**渲染结果必须来自 GPU**」的**可验证断言** ✓**** ✓✓
{
  const seen = [auto, off, on].map((h) => `${h.mode}:${h.backend}`);
  console.log(`  render_backend 三种模式：${seen.join("｜")}`);
  const allCpu = [auto, off, on].every((h) => h.backend === "cpu");
  check(allCpu,
    "服务端尚无 GPU 渲染路径 ⇒ `render_backend` 必须恒为 `cpu`（不许报 gpu 冒充）",
    `实测 ${seen.join("｜")}`);
}

// **★ §6.3 的 ④：`max_channel_delta` 必须**诚实** ✗ ★**（第 293 轮 ✓）：
//   **∴ 规则 ✗**：**没比过 ⇒ **必须 `null`**✗ ⇒ **∴ 不许**报 `0`** ✓
//     （**∴ 因为** `0` 读起来像「**比过且逐位相同**」⇒ **∴ 那是**撒谎** ✓）** ✓✓
//   **∴ 而**比过 ⇒ **必须是**数字**（**`0` 表示逐位相同 ✓）** ✓✓
//   **∴ 变异点 ✗**：**无 feature 时返回 `0`**✗ ⇒ **∴ 本断言**必红** ✓**** ✓✓
console.log("");
console.log(`  max_channel_delta：auto=${JSON.stringify(auto.delta)}｜note=${auto.note ?? "(无)"}`);
{
  const deltas = [auto, off, on].map((h) => h.delta);
  const allNull = deltas.every((d) => d === null);
  const allNum = deltas.every((d) => typeof d === "number");
  check(
    allNull || allNum,
    "`max_channel_delta` 必须**要么全是 null（**没比过 ✓）、要么全是数字（**比过 ✓）**—— **不许**混**",
    `实测 ${JSON.stringify(deltas)}`
  );
  if (allNum) {
    check(
      deltas.every((d) => d >= 0 && d <= 255),
      "若报数字 ⇒ 必须落在 [0, 255]（**通道差的合法范围 ✓）",
      `实测 ${JSON.stringify(deltas)}`
    );
    check(
      typeof auto.note === "string" && auto.note.length > 0,
      "报数字时必须**同时**给出 note（**∴ 让读数**自解释** ✓）",
      `note=${auto.note ?? "(空)"}`
    );
  } else {
    check(
      typeof auto.note === "string" && auto.note.includes("未做"),
      "报 `null` 时 note 必须**明说**没做比对",
      `note=${auto.note ?? "(空)"}`
    );
  }
}

console.log("");
if (failed === 0) { console.log("  结论：✓ GPU 的「不可用原因」与后端自洽"); process.exit(0); }
console.error(`  结论：✗ ${failed} 条 ⇒ **∴ 目标第 7 条**不许假装／不许沉默降级**`);
process.exit(1);
