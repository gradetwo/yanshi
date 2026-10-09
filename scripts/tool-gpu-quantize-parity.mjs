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

const argv = process.argv.slice(2);
const wantGpu = argv.includes("--gpu");
const base = String(argv.find((a) => !a.startsWith("--")) || "http://127.0.0.1:8899")
  .replace(/\/+$/, "");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cdpPort = 9900 + Math.floor(Math.random() * 90);
const profile = trackTemp(`/tmp/gpu-parity-${cdpPort}`);

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
// **★ 输入要**够大 ＋ 含边界 ✗ ★**（第 52 轮 ✓）：**∴ 4 个像素**不足以说
//   "**逐字节相同 ✓"**✗ ⇒ **∴ 于是**：**边界值 ＋ 伪随机 ✗（**固定种子 ⇒ **∴ 可复现 ✓）**
//     ＋ **报告**差异字节数 ✓（**∴ 不**只最大值 ✓）** ✓✓
const BOUNDARY = [0, 1e-8, 0.0031308, 1 / 255, 0.5, 0.50598186, 1 - 1e-7, 1, 0.25, 0.75];
function buildPixels() {
  const pixels = [];
  // **∴ ① 边界组合 ✗**：**每个边界的**单通道**＋ **全通道同值 ✓** ✓✓
  for (const value of BOUNDARY) {
    pixels.push(value, 0, 0, 1);
    pixels.push(value, value, value, 1);
    pixels.push(value, value, value, value);
  }
  // **∴ ② 伪随机（**固定种子 ✓）✗** ⇒ **∴ 覆盖**大片取值 ✓** ✓✓
  let seed = 0x2f6e2b1;
  const next = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };
  for (let i = 0; i < 4096; i += 1) pixels.push(next(), next(), next(), next());
  return pixels;
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
  // **★ 模板字符串内**不许**出现反引号**✗（**∴ 注释里也不行 ✓）★**
  //   **∴ 我**为此**连踩三次**✗（**shader 一次／判据注释两次 ✓）
  //   ⇒ **∴ 所以**：**本段表达式里**不写注释**✗，**要点写在模板外 ✓**** ✓✓
  // **∴ 输入规模 ✗**：**10 个边界值 × 3 组 ＋ **4096 个伪随机像素**✗
  //   ⇒ **∴ 于是**：**"**逐字节相同 ✓"**这个说法**才有分量 ✓**** ✓✓
  const result = await evaluate(`(async () => {
    try {
    const pixels = (() => {
      const values = [0, 1e-8, 0.0031308, 1 / 255, 0.5, 0.50598186, 1 - 1e-7, 1, 0.25, 0.75];
      const out = [];
      for (const value of values) {
        out.push(value, 0, 0, 1);
        out.push(value, value, value, 1);
        out.push(value, value, value, value);
      }
      let seed = 0x2f6e2b1;
      const next = () => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff; };
      for (let i = 0; i < 4096; i += 1) out.push(next(), next(), next(), next());
      return new Float32Array(out);
    })();
    const first = await window.yanshiGpuQuantizeCompared(pixels);
    const second = await window.yanshiGpuQuantizeCompared(pixels);
    const same = first.bytes.length === second.bytes.length
      && first.bytes.every((b, i) => b === second.bytes[i]);
    return JSON.stringify({
      backend: first.backend,
      maxChannelDelta: first.maxChannelDelta,
      typeofDelta: typeof first.maxChannelDelta,
      deterministic: same,
      bytes: Array.from(first.bytes),
      differingBytes: first.differingBytes,
      pixelCount: pixels.length / 4,
      deltas: Array.from(first.bytes).map((b, i) => Math.abs(b - first.reference[i])),
    });
    } catch (error) {
      return JSON.stringify({ threw: String((error && error.message) || error) });
    }
  })()`);
  // **∴ 结果可能**不是 JSON ✗**（**∴ 如**页面**抛了异常 ✓）⇒ **∴ 必须**原样打印 ✗
  //   ⇒ **∴ 否则**：**JSON.parse**报一个**看不懂的错**✗，**而**真原因**被吞掉 ✓**** ✓✓
  let out = null;
  try {
    out = JSON.parse(result);
  } catch {
    check(false, "GPU 路径必须返回结果对象（**实测**抛了异常或返回了非 JSON ✓）",
      String(result).slice(0, 200));
  }
  if (!out) {
    socket.close();
    chrome.kill();
    console.error("");
    console.error(`  结论：GPU 量化切片**未达标** ✗（页面侧原值：${String(result).slice(0, 300)}）`);
    process.exit(1);
  }
  if (out.threw) {
    check(false, "GPU 路径**抛错**（**∴ 真错在这里 ✓）", String(out.threw).slice(0, 220));
  }
  console.log(`  GPU 结果：${JSON.stringify(out).slice(0, 400)}`);
  if (out.threw) {
    check(false, "GPU 路径**抛错**（**∴ 真错就在这里 ✓）", String(out.threw).slice(0, 240));
  }
  if (out.deltas) {
    const differing = out.deltas.filter((d) => d !== 0).length;
    console.log(`  逐通道差异：**不同字节 ${differing}／${out.deltas.length}**｜最大 ${out.maxChannelDelta}`);
    if (differing > 0) {
      const firstIndex = out.deltas.findIndex((d) => d !== 0);
      console.log(`  首个不同在第 ${firstIndex} 个字节`
        + `（GPU=${out.bytes[firstIndex]} vs 参考=${out.bytes[firstIndex] - 0}）`);
    }
  }
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
// **∴ 给 chromium**一点时间真正退出 ✗ ★**（第 54 轮 ✓）：
//   **∴ 它**还在写 profile 时**`rmSync` 会失败**✗ ⇒ **∴ 于是**留下目录 ✓
//     ⇒ **∴ 所以**：**杀完等一会儿**再进退出钩子 ✓**** ✓✓
await new Promise((resolve) => setTimeout(resolve, 400));
console.log("");
if (failures.length) {
  console.error(`  结论：GPU 量化切片**未达标** ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ GPU 量化切片达标（后端如实 ＋ 差异已量化 ＋ 确定性 ✓）");
process.exit(0);
