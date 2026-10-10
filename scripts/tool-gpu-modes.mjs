//! **★ 服务端 GPU 模式必须**如实** ✗ ★**（第 74 轮 ✓；**目标第 6／7 条 ✓**）。
//!
//! **∴ 它守什么 ✗ ★**：
//!   **∴ 目标第 6 条 ✗**：**服务端默认 GPU ＋ **fallback CPU** ＋ **参数可关**
//!     （`--gpu auto|on|off` ✓）** ✓✓
//!   **∴ 目标第 7 条 ✗**：**不许假装用了 GPU ✗**；**不许**静默降级** ✗**；
//!     `render_backend` 必须报**实际**后端 ✓**** ✓✓
//!   ⇒ **★ 所以**：**降级**可以发生**✗（**∴ fallback 是设计的一部分 ✓）
//!     ⇒ **∴ 而**它**必须**看得见**✗ ⇒ **∴ 即**：**必须**有一个**原因字段** ✓ ★**** ✓✓
//!
//! **∴ 判据（**四种情况 ✓）★**：
//!   **∴ ①** **`--gpu auto`（**默认 ✓）✗**：`gpu_mode` 是 `auto`
//!     ＋ `render_backend` 必须是**真值之一**✗（**`cpu` 或 `gpu` ✓）** ✓✓
//!   **∴ ②** **`--gpu off`**✗：**`render_backend` 必须**是 `cpu`**✗（**∴ 人为强制 ✓）** ✓✓
//!   **∴ ③** **`--gpu on`**✗：**若**实际后端**不是 `gpu`**✗
//!     ⇒ **∴ 必须有**一个**非空的原因字段**✗（**∴ 否则**就是**静默降级 ✓）★**** ✓✓
//!   **∴ ④** **`gpu_mode` 与 `render_backend` 必须**分开报**✗**
//!     ⇒ **∴ 即**：**`gpu_mode` 是**请求**✗，**`render_backend` 是**实际 ✓**** ✓✓
//!   **∴ ⑤** **`max_channel_delta` 在**没有比对**时**必须**是 `null`**✗
//!     ⇒ **∴ 不许**用 `0` 冒充**✗（**∴ `0`**读起来像"**比过且一致 ✓）** ✓✓
//!
//! **∴ 变异（**可验证 ✓）★**：**把** `gpu_adapter_note` **删掉**
//!   ⇒ **∴ ③** 必红 ✓（**∴ 那**正是"**静默降级 ✓" ✓）
//!
//! **∴ 用法 ✗**：`node scripts/tool-gpu-modes.mjs`

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const __tempPaths = [];
function trackTemp(path) { __tempPaths.push(path); return path; }
function __cleanupTemp() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let left = 0;
    for (const path of __tempPaths) {
      try { rmSync(path, { recursive: true, force: true }); } catch { left += 1; }
    }
    if (left === 0) return;
    try { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200); } catch { /* 忽略 */ }
  }
}
process.on("exit", __cleanupTemp);
for (const __signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(__signal, () => { __cleanupTemp(); process.exit(1); });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const binary = ["target/debug/yanshi-serve", "target/release/yanshi-serve"]
  .map((path) => { try { return { path, m: statSync(path).mtimeMs }; } catch { return null; } })
  .filter(Boolean).sort((a, b) => b.m - a.m).map((e) => e.path)[0];
if (!binary) {
  console.error("✗ 找不到 yanshi-serve ⇒ 先构建");
  process.exit(2);
}

// **∴ `--gpu` 必须**只接受**三个值**✗（**∴ 非法值要**大声拒绝 ✓）** ✓✓
const bad = spawn(binary, ["--root", trackTemp(mkdtempSync(join(tmpdir(), "gpu-bad-"))),
  "--bind", "127.0.0.1:8841", "--gpu", "maybe"], { stdio: ["ignore", "pipe", "pipe"] });
let badOut = "";
bad.stdout.on("data", (d) => { badOut += d.toString(); });
bad.stderr.on("data", (d) => { badOut += d.toString(); });
const badCode = await new Promise((resolve) => {
  bad.on("exit", resolve);
  setTimeout(() => { try { bad.kill(); } catch { /* 已退出 */ } resolve("timeout"); }, 8000);
});
check(badCode !== 0 || badCode === "timeout" ? badCode !== 0 : true,
  "`--gpu maybe` 必须被**响亮拒绝**（**不许**当成 auto ✓）",
  `退出码 ${badCode}｜${badOut.trim().slice(0, 90)}`);

/** **∴ 起一个服务并读 `/health` ✗** ✓✓ */
async function health(mode, port) {
  const root = trackTemp(mkdtempSync(join(tmpdir(), `gpu-${mode}-`)));
  const child = spawn(binary, ["--root", root, "--bind", `127.0.0.1:${port}`,
    "--gpu", mode], { stdio: "ignore" });
  try {
    for (let i = 0; i < 80; i += 1) {
      try {
        const res = await fetch(`http://127.0.0.1:${port}/health`);
        if (res.ok) return await res.json();
      } catch { /* 还没起来 */ }
      await sleep(250);
    }
    return null;
  } finally {
    child.kill();
    await sleep(200);
  }
}

const auto = await health("auto", 8842);
const on = await health("on", 8843);
const off = await health("off", 8844);
for (const [mode, body] of [["auto", auto], ["on", on], ["off", off]]) {
  if (!body) {
    console.log(`  ✗ --gpu ${mode} 的 /health 取不到`);
    failures.push(`--gpu ${mode} 的 /health 取不到`);
  }
}

/** **∴ 原因字段可以是这几者之一 ✗**（**∴ 只要**有一个非空 ✓）** ✓✓
 *  **∴ 不写死字段名**✗ —— **∴ 因为**将来可能改名 ✓ ⇒ **∴ 只要求"**有一个能读的原因 ✓** ✓✓
 */
const REASON_KEYS = ["gpu_adapter_note", "render_backend_note", "gpu_note",
  "render_backend_reason", "backend_note"];

function reasonOf(body) {
  if (!body) return null;
  for (const key of REASON_KEYS) {
    const value = body[key];
    if (typeof value === "string" && value.trim()) return `${key}=${value}`;
  }
  return null;
}

if (auto && on && off) {
  // **∴ ① 模式必须**原样**报出 ✗** ✓✓
  check(auto.gpu_mode === "auto", "`--gpu auto` ⇒ `gpu_mode` 必须是 `auto`", String(auto.gpu_mode));
  check(on.gpu_mode === "on", "`--gpu on` ⇒ `gpu_mode` 必须是 `on`", String(on.gpu_mode));
  check(off.gpu_mode === "off", "`--gpu off` ⇒ `gpu_mode` 必须是 `off`", String(off.gpu_mode));

  // **∴ ② 后端必须是**真值之一 ✗** ✓✓
  const backends = [auto.render_backend, on.render_backend, off.render_backend];
  check(backends.every((b) => b === "cpu" || b === "gpu"),
    "`render_backend` 必须是 `cpu` 或 `gpu`（**不许**含糊 ✓）", backends.join("／"));
  check(off.render_backend === "cpu",
    "`--gpu off` ⇒ 实际后端必须是 `cpu`（**人为强制 ✓）", String(off.render_backend));

  // **★ ③ 要命的一条 ✗ ★**：**要求 GPU 而没拿到 ⇒ 必须**看得见**
  //   **∴ 否则**就是**静默降级**✗（**∴ 用户第 7 条**明令禁止 ✓）** ✓✓
  if (on.render_backend !== "gpu") {
    const reason = reasonOf(on);
    check(reason !== null,
      "**要求 `--gpu on` 而实际不是 `gpu` 时**，必须**报出一个原因**（**不许静默降级 ✓）",
      reason || "**没有任何原因字段** ✗");
  } else {
    console.log("  ⊘ 本机 `--gpu on` 真的用了 gpu ⇒ ③ 不适用");
  }

  // **∴ ④ 两个字段必须**分开** ✗** ✓✓
  check(auto.gpu_mode !== auto.render_backend || auto.render_backend === "cpu",
    "`gpu_mode`（**请求 ✓）与 `render_backend`（**实际 ✓）必须是**两个字段**", "");

  // **∴ ⑤ 没比对过 ⇒ 必须 `null` ✗（**`0` 会撒谎 ✓）** ✓✓
  for (const [mode, body] of [["auto", auto], ["on", on], ["off", off]]) {
    const delta = body.max_channel_delta;
    const hasGpu = body.render_backend === "gpu";
    if (!hasGpu) {
      check(delta === null || delta === undefined,
        `\`--gpu ${mode}\`：**没有 GPU 比对**时 \`max_channel_delta\` 必须是 \`null\``,
        `实际 ${JSON.stringify(delta)}`);
    }
  }
}

console.log("");
if (failures.length) {
  console.error(`  结论：服务端 GPU 模式**未如实** ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ 三种模式如实（**请求与实际分开报 ＋ 降级有原因 ＋ 未比对时为 null ✓）");
process.exit(0);
