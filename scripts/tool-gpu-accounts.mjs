//! **★ GPU 收益的**两本账**（**浏览器内核 ✓）✗ ★**（第 55 轮 ✓；**用户第 594 轮 ✓**）。
//!
//! **∴ 依据 ✗**：**用户第 594 轮**要求 GPU 的收益**必须**记两本账**✗：
//!   **∴ ① 时间账 ✗**：**墙钟**
//!   **∴ ② CPU 占用账 ✗**：**同一场景**下的 CPU 时间**✗
//!     ⇒ **∴ 浏览器内核**用 **CDP `Performance.getMetrics` 的 `TaskDuration`** 当代理 ✓**** ✓✓
//!   **∴ 且 ✗**：**若**只有墙钟变好**而** CPU 没降（**或更差 ✓）⇒ **∴ 必须**明说 ✓**** ✓✓
//!
//! **∴ 量什么 ✗**：**同一份输入**（**大块像素 ✓）走两条路**✗：
//!   **∴ CPU 路径 ✗**：**内核里的 `quantizeOnCpu`**（**同步 ✓）
//!   **∴ GPU 路径 ✗**：**`yanshiGpuQuantize`**（**WebGPU ✓）
//!   ⇒ **∴ 每条**都记**墙钟 ＋ **主线程 `TaskDuration` 增量** ✓**** ✓✓
//!
//! **∴ 为什么**主线程时间的下降**才是要点 ✗ ★**：
//!   **∴ GPU**的价值**不在**墙钟**✗（**∴ 大块数据**受**传输**限制 ✓）
//!     ⇒ **∴ 而**在于**把逐像素工作**从**主线程**搬走**✗
//!       ⇒ **∴ 所以**：**TaskDuration** 会**明显下降**✗，**而**墙钟**可能**几乎不变 ✓**** ✓✓
//!     ⇒ **★ 那**正是**用户强调"**也要算降 CPU 占用 ✓"**的原因 ✓ ★**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/tool-gpu-accounts.mjs <base-url> [--gpu] [--pixels N]`

import { spawn } from "node:child_process";
import { rmSync } from "node:fs";

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

const argv = process.argv.slice(2);
const wantGpu = argv.includes("--gpu");
const pixelOption = argv.indexOf("--pixels");
// **∴ 默认 4 M 像素 ✗**（**∴ 16 MB 输入 ✓）⇒ **∴ 足够**让 GPU 的固定开销**被摊薄 ✓**** ✓✓
const PIXELS = pixelOption >= 0 ? Number(argv[pixelOption + 1]) : 1024 * 1024;
const base = String(argv.find((a) => !a.startsWith("--") && !/^\d+$/.test(a))
  || "http://127.0.0.1:8899").replace(/\/+$/, "");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cdpPort = 9600 + Math.floor(Math.random() * 90);
const profile = trackTemp(`/tmp/gpu-accounts-${cdpPort}`);

const flags = [
  "--headless=new", `--remote-debugging-port=${cdpPort}`, "--no-sandbox",
  "--disable-dev-shm-usage", `--user-data-dir=${profile}`,
];
if (wantGpu) {
  flags.push("--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=vulkan");
} else {
  flags.push("--ignore-gpu-blocklist");
}
flags.push("about:blank");

const chrome = spawn(process.env.CHROME_BIN || "chromium", flags, { stdio: "ignore" });
let targets = null;
for (let i = 0; i < 80; i += 1) {
  await sleep(300);
  try {
    targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
    if (targets && targets.length) break;
  } catch { /* 还没起来 */ }
}
if (!targets || !targets.length) {
  console.error("❌ chromium 未就绪 ⇒ 本次实测无效");
  chrome.kill();
  process.exit(2);
}

const page = targets.find((t) => t.type === "page") || targets[0];
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const resolve = pending.get(message.id);
    pending.delete(message.id);
    resolve(message.result || message.error || {});
  }
});
await new Promise((resolve) => socket.addEventListener("open", resolve));
const send = (method, params) => {
  const id = nextId++;
  socket.send(JSON.stringify({ id, method, params: params || {} }));
  return new Promise((resolve) => pending.set(id, resolve));
};
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", {
    expression, returnByValue: true, awaitPromise: true,
  });
  if (result.exceptionDetails) {
    return `THREW:${result.exceptionDetails.exception?.description
      || result.exceptionDetails.text}`;
  }
  return result.result ? result.result.value : undefined;
};
const taskDuration = async () => {
  const metrics = await send("Performance.getMetrics");
  for (const item of metrics.metrics || []) {
    if (item.name === "TaskDuration") return item.value;
  }
  return null;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Performance.enable");
await send("Page.navigate", { url: "about:blank" });
await evaluate(`(async () => {
  if (navigator.serviceWorker) {
    for (const reg of await navigator.serviceWorker.getRegistrations()) await reg.unregister();
  }
  if (window.caches) { for (const key of await caches.keys()) await caches.delete(key); }
})()`);
await send("Page.navigate", { url: `${base}/` });
let ready = false;
for (let i = 0; i < 120; i += 1) {
  await sleep(250);
  ready = await evaluate("typeof window.yanshiGpuQuantize === 'function'");
  if (ready === true) break;
}

const gpuFact = await evaluate(`(async () => {
  const has = !!navigator.gpu;
  let adapter = false, reason = null;
  if (has) {
    try { const a = await navigator.gpu.requestAdapter();
      adapter = !!a; if (!a) reason = "requestAdapter() 返回 null";
    } catch (e) { reason = String(e && e.message || e); }
  } else { reason = "navigator.gpu 不存在"; }
  return JSON.stringify({ has, adapter, reason });
})()`);
const fact = JSON.parse(gpuFact);

// **∴ 两条路各量一次 ✗**（**∴ 同一份输入 ＋ **先各跑一次预热 ✓）** ✓✓
const measure = async (label) => {
  // **∴ 页面内**先造输入（**∴ 固定种子 ⇒ **两边同一份 ✓）** ✓✓
  const setup = await evaluate(`(() => {
    let seed = 0x1234567;
    const next = () => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; };
    const pixels = new Float32Array(${PIXELS} * 4);
    for (let i = 0; i < pixels.length; i += 1) pixels[i] = next();
    window.__gpuAccountPixels = pixels;
    return pixels.length;
  })()`);
  if (typeof setup === "number" && setup === PIXELS * 4) {
    // 输入就绪
  }
  const expression = label === "cpu"
    ? `(async () => {
        const pixels = window.__gpuAccountPixels;
        const started = performance.now();
        const bytes = window.yanshiGpuQuantizeCpu ? window.yanshiGpuQuantizeCpu(pixels) : null;
        return JSON.stringify({ ok: !!bytes, ms: performance.now() - started });
      })()`
    : `(async () => {
        const pixels = window.__gpuAccountPixels;
        const started = performance.now();
        try {
          const r = await window.yanshiGpuQuantize(pixels);
          return JSON.stringify({ ok: true, ms: performance.now() - started,
            maxChannelDelta: r.maxChannelDelta, backend: r.backend });
        } catch (error) {
          return JSON.stringify({ ok: false, ms: performance.now() - started,
            error: String((error && error.message) || error) });
        }
      })()`;
  const before = await taskDuration();
  const raw = await evaluate(expression);
  const after = await taskDuration();
  let out = null;
  try { out = JSON.parse(raw); } catch { out = { ok: false, error: String(raw) }; }
  return {
    label,
    wallMs: out.ms,
    taskMs: before !== null && after !== null ? (after - before) * 1000 : null,
    ok: out.ok,
    detail: out,
  };
};

console.log("");
console.log(`  目标：${base}｜像素：${PIXELS}（**输入 ${(PIXELS * 16 / 1048576).toFixed(1)} MB ✓）`);
console.log(`  WebGPU：navigator.gpu=${fact.has}｜adapter=${fact.adapter}`
  + `${fact.reason ? "｜原因=" + fact.reason : ""}`);
if (!ready) {
  console.error("  ✗ 页面没有内核入口 ⇒ 本次实测无效（先重新同步 web/）");
  socket.close(); chrome.kill();
  await sleep(400);
  process.exit(2);
}

// **∴ 预热 ✗**（**∴ 冷启动**会**污染第一轮 ✓）** ✓✓
await measure("cpu");
if (fact.adapter) await measure("gpu");

const cpu = await measure("cpu");
const gpu = fact.adapter ? await measure("gpu") : null;

console.log("");
console.log("  ★ 两本账（浏览器内核）★");
console.log("  | 路径 | ① 墙钟 ms | ② 主线程 TaskDuration ms | 结果 |");
console.log("  |---|---|---|---|");
console.log(`  | CPU | ${cpu.wallMs?.toFixed(1)} | ${cpu.taskMs?.toFixed(1)} | `
  + `${cpu.ok ? "✓" : "✗ " + String(cpu.detail?.error || "").slice(0, 60)} |`);
if (gpu) {
  console.log(`  | GPU | ${gpu.wallMs?.toFixed(1)} | ${gpu.taskMs?.toFixed(1)} | `
    + `${gpu.ok ? `✓ maxChannelDelta=${gpu.detail.maxChannelDelta}` : "✗ " + String(gpu.detail?.error || "").slice(0, 60)} |`);
}

// **★ 必须**两本账都明说 ✗ ★**（**∴ 不许**只报好看的那本 ✓）** ✓✓
if (!gpu) {
  console.log("");
  console.log("  ℹ️ 没有适配器 ⇒ **∴ 只有**CPU 一本账**✗（**∴ 不**假装有 GPU ✓）**");
} else {
  const wallGain = cpu.wallMs && gpu.wallMs ? (1 - gpu.wallMs / cpu.wallMs) * 100 : null;
  const cpuGain = cpu.taskMs && gpu.taskMs ? (1 - gpu.taskMs / cpu.taskMs) * 100 : null;
  console.log("");
  console.log(`  ∴ 时间账：GPU ${wallGain === null ? "（无法比）" : (wallGain > 0 ? "快 " : "慢 ") + Math.abs(wallGain).toFixed(1) + "%"}`);
  console.log(`  ∴ CPU 占用账：GPU 的主线程时间 ${cpuGain === null ? "（无法比）" : (cpuGain > 0 ? "少 " : "多 ") + Math.abs(cpuGain).toFixed(1) + "%"}`);
  if (cpuGain !== null && cpuGain < 0 && wallGain !== null && wallGain > 0) {
    console.log("  ⚠️ **只有墙钟变好而 CPU 占用更差** ⇒ **∴ 按第 8 条必须明说** ✓");
  }
}

socket.close();
chrome.kill();
await sleep(400);
process.exit(0);
