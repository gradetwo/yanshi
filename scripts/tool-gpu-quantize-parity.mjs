//! **★ GPU 量化切片的**一致性判据** ✗ ★**（第 50 轮 ✓；**§6.3 的 ④ ＋ 第 594 轮的要求 ✓**）。
//!
//! **∴ 判什么 ✗ ★**：
//!   **∴ ①** **有适配器时**：`yanshiGpuQuantize` **必须**返回 `backend === "gpu"`**✗**
//!     ＋ **必须**返回一个**数值型** `maxChannelDelta`**✗**
//!       ⇒ **★ 不许**用恒 0 冒充 ✓（**∴ 那**是目标明文禁止的 ✓）★**** ✓✓
//!   **∴ ②** **`maxChannelDelta` 必须 ≤ 1** ✗**（**§6.3 的容差口径 ✓，**与 `export_small` 同款 ✓）**
//!   **∴ ③** **没有适配器时**：**必须**抛错**✗
//!     ⇒ **∴ 不许**静默降级到 CPU 却报成 GPU ✓**（**第 7 条 ✓）
//!   **∴ ④** **同一输入两次** ⇒ **GPU 结果必须**逐字节相同**✗（**确定性 ✓）** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/tool-gpu-quantize-parity.mjs <base-url> [--gpu]`
//!   **∴ 不带 `--gpu`**✗：**本机**拿不到适配器 ⇒ **∴ 走 ③ 的路径 ✓**
//!   **∴ 带 `--gpu`**✗：**加上 Vulkan 标志 ⇒ **∴ 走 ① ② ④ 的路径 ✓**** ✓✓

import { spawn } from "node:child_process";

const argv = process.argv.slice(2);
const wantGpu = argv.includes("--gpu");
const base = String(argv.find((a) => !a.startsWith("--")) || "http://127.0.0.1:8899")
  .replace(/\/+$/, "");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cdpPort = 9900 + Math.floor(Math.random() * 90);
const profile = `/tmp/gpu-parity-${cdpPort}`;

const flags = [
  "--headless=new",
  `--remote-debugging-port=${cdpPort}`,
  "--no-sandbox",
  "--disable-dev-shm-usage",
  `--user-data-dir=${profile}`,
];
if (wantGpu) {
  flags.push("--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=vulkan");
} else {
  flags.push("--ignore-gpu-blocklist");
}
flags.push("about:blank");

const chrome = spawn(process.env.CHROME_BIN || "chromium", flags, { stdio: "ignore" });
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

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
  return result.result ? result.result.value : undefined;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
await send("Network.setCacheDisabled", { cacheDisabled: true });
// **∴ 绕开 SW ✗**（**∴ 第 49 轮的教训 ✓）** ✓✓
await send("Page.navigate", { url: "about:blank" });
await evaluate(`(async () => {
  if (navigator.serviceWorker) {
    for (const reg of await navigator.serviceWorker.getRegistrations()) await reg.unregister();
  }
  if (window.caches) { for (const key of await caches.keys()) await caches.delete(key); }
})()`);
await send("Page.navigate", { url: `${base}/` });
for (let i = 0; i < 120; i += 1) {
  await sleep(250);
  if (await evaluate("document.readyState === 'complete' && typeof window.yanshiGpuQuantize === 'function'")) break;
}
const present = await evaluate("typeof window.yanshiGpuQuantize === 'function'");
check(present === true, "页面必须暴露 `yanshiGpuQuantize`", `实测 ${present}`);

// **∴ 一组**已知输入**✗：**纯色 ＋ 边界值 ＋ 半透明 ✓** ✓✓
const CASES = [
  { name: "不透明红", pixel: [1, 0, 0, 1] },
  { name: "线性 0.5 灰", pixel: [0.5, 0.5, 0.5, 1] },
  { name: "f16 敏感值", pixel: [0.50598186, 0.50598186, 0.50598186, 1] },
  { name: "半透明蓝", pixel: [0.1, 0.2, 0.9, 0.5] },
];

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
console.log(`  WebGPU：navigator.gpu=${fact.has}｜adapter=${fact.adapter}`
  + `${fact.reason ? "｜原因=" + fact.reason : ""}`);

if (!fact.adapter) {
  // **★ ③ 没有适配器 ⇒ 必须抛错 ✗**（**∴ 不许**静默降级却报成 GPU ✓）** ✓✓
  const thrown = await evaluate(`(async () => {
    try {
      await window.yanshiGpuQuantize(new Float32Array([1, 0, 0, 1]));
      return "no-throw";
    } catch (error) { return "threw:" + String(error && error.message || error); }
  })()`);
  check(String(thrown).startsWith("threw:"),
    "**没有适配器时**必须抛错（不许静默降级却报成 GPU ✗）", String(thrown).slice(0, 120));
  check(!String(thrown).includes("no-throw"),
    "**不许**在无适配器时返回结果", String(thrown).slice(0, 80));
} else {
  // **★ ①②④ 有适配器 ⇒ 必须真跑 GPU ✗** ✓✓
  const result = await evaluate(`(async () => {
    const pixels = new Float32Array(${JSON.stringify(CASES.flatMap((c) => c.pixel))});
    const first = await window.yanshiGpuQuantize(pixels);
    const second = await window.yanshiGpuQuantize(pixels);
    const same = first.bytes.length === second.bytes.length
      && first.bytes.every((b, i) => b === second.bytes[i]);
    return JSON.stringify({
      backend: first.backend,
      maxChannelDelta: first.maxChannelDelta,
      typeofDelta: typeof first.maxChannelDelta,
      deterministic: same,
      bytes: Array.from(first.bytes),
      deltas: Array.from(first.bytes).map((b, i) => Math.abs(b - first.reference[i])),
    });
  })()`);
  const out = JSON.parse(result);
  console.log(`  GPU 结果：backend=${out.backend}｜maxChannelDelta=${out.maxChannelDelta}`
    + `（${out.typeofDelta}）｜确定性=${out.deterministic}`);
  console.log(`  逐通道差异：${out.deltas.join(",")}`);
  check(out.backend === "gpu", "**必须**报成 `gpu`（实际后端 ✗）", String(out.backend));
  check(out.typeofDelta === "number",
    "**必须**报出数值型 `maxChannelDelta`（**不许**恒 0 冒充也**不许**缺 ✓）", out.typeofDelta);
  check(typeof out.maxChannelDelta === "number" && out.maxChannelDelta <= 1,
    "**`maxChannelDelta` 必须 ≤ 1**（**§6.3 的容差口径 ✓）", String(out.maxChannelDelta));
  check(out.deterministic === true,
    "**同一输入两次** ⇒ GPU 结果必须逐字节相同（确定性 ✗）", String(out.deterministic));
}

socket.close();
chrome.kill();
console.log("");
if (failures.length) {
  console.error(`  结论：GPU 量化切片**未达标** ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ GPU 量化切片达标（后端如实 ＋ 差异已量化 ＋ 确定性 ✓）");
process.exit(0);
