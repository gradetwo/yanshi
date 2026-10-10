#!/usr/bin/env node
// **★ 内核的异步 GPU 路必须是**真的异步**✗ ★**（**第 481 轮 ✓；**目标第 6／7 条 ✓）
//
// **∴ 为什么需要这条判据 ✗**：**第 470 轮实测过 ✗**：
//   **∴ 同步等待（**`block_on`／`poll(Wait)` ✓）在 wasm32 上**会死锁**✗
//     ⇒ **∴ 于是**：**内核的 GPU 入口**只能是 `async`** ✓
//       ＋ **∴ 而**「**写了 `async fn`**」**不等于**「**真的 await**」✗
//         ⇒ **∴ 常见的假修 ✗**：**外壳是 `async`**✗
//           ＋ **∴ 而**内部仍调**同步**版本** ✓
//             ⇒ **∴ 于是**：**它**在浏览器里**照样死锁** ✓ ★**** ✓✓
//
// **∴ 本条判据守什么 ✗**：
//   ① `gpu_probe_async` 与 `gpu_quantize_async` **存在** ＋ **带 `#[wasm_bindgen]`**
//   ② **它们**走 `new_async`／`quantize_async`**✗ ＋ **带 `.await`**
//   ③ **内核的 GPU 块里**不许出现 `block_on`**✗（**∴ 那是假修的标志 ✓）
//   ④ `Cargo.toml` 的 `gpu` feature **三项齐全**
//      （**`yanshi-render/gpu` ＋ `dep:yanshi-gpu` ＋ `dep:wasm-bindgen-futures` ✓）
//   ⑤ **默认 feature** 不含 gpu**✗（**∴ 体积**不受影响 ✓）
//
// **∴ 变异（**可验证 ✓）★**：
//   **∴ ①** 删掉一个 `.await` ⇒ **必须退 1**
//   **∴ ②** 在内核 GPU 块里塞一个 `block_on(` ⇒ **必须退 1**
//   **∴ ③** 把 `gpu` 从 `default` 里去掉（**或反之**）⇒ **必须退 1**
//
// **∴ 退出码 ✗**：**0 ＝ 通过**✗｜**1 ＝ 产品缺陷**✗｜**3 ＝ 判据自身跑不了**
//   （**∴ 按 AGENTS.md：**判据要能说「**我自己坏了**」 ✓）★**** ✓✓

import { readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const ROOT = process.cwd();
const WASM_LIB = join(ROOT, "crates/yanshi-wasm/src/lib.rs");
const WASM_TOML = join(ROOT, "crates/yanshi-wasm/Cargo.toml");

// **∴ 判据自身的前提 ✗**（**∴ 缺了就是「**不可判**」✗ ⇒ **∴ 退 3 ✓）
for (const p of [WASM_LIB, WASM_TOML]) {
  if (!existsSync(p)) {
    console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：${p}`);
    process.exit(3);
  }
}

const lib = readFileSync(WASM_LIB, "utf8");
const toml = readFileSync(WASM_TOML, "utf8");

let failed = 0;
const check = (ok, msg) => {
  console.log(`  ${ok ? "✓" : "✗"} ${msg}`);
  if (!ok) failed += 1;
};

console.log("  ── ① 两个异步导出必须存在且被 wasm_bindgen 导出 ──");
check(/pub async fn gpu_probe_async\(/.test(lib), "gpu_probe_async 必须是 pub async fn");
check(/pub async fn gpu_quantize_async\(/.test(lib), "gpu_quantize_async 必须是 pub async fn");

// **∴ 两个导出**都要在 `#[wasm_bindgen]` 之下**✗（**∴ 用**就近窗口**判断 ✓）
const nearAttr = (name, attr, window = 400) => {
  const i = lib.indexOf(`pub async fn ${name}`);
  if (i < 0) return false;
  return lib.slice(Math.max(0, i - window), i).includes(attr);
};
check(nearAttr("gpu_probe_async", "#[wasm_bindgen]"), "gpu_probe_async 上方必须有 #[wasm_bindgen]");
check(nearAttr("gpu_quantize_async", "#[wasm_bindgen]"), "gpu_quantize_async 上方必须有 #[wasm_bindgen]");
check(
  nearAttr("gpu_probe_async", '#[cfg(feature = "gpu")]'),
  'gpu_probe_async 必须被 #[cfg(feature = "gpu")] 关住（**默认产物不许带上它**）',
);
check(
  nearAttr("gpu_quantize_async", '#[cfg(feature = "gpu")]'),
  'gpu_quantize_async 必须被 #[cfg(feature = "gpu")] 关住',
);

console.log("  ── ② 真的 await，而不是只写一个 async 外壳 ──");
const bodyOf = (name) => {
  const i = lib.indexOf(`pub async fn ${name}`);
  if (i < 0) return "";
  // **∴ 取到**函数结尾**（**∴ 用**下一个顶层 `}` 之后的行**做粗切 ✓）
  return lib.slice(i, i + 2500);
};
const probeBody = bodyOf("gpu_probe_async");
const quantBody = bodyOf("gpu_quantize_async");
check(/new_async\([^)]*\)[\s\S]{0,40}\.await/.test(probeBody), "gpu_probe_async 必须 await new_async(…)");
check(
  /quantize_async\([^)]*\)[\s\S]{0,40}\.await/.test(quantBody),
  "gpu_quantize_async 必须 await quantize_async(…)",
);

console.log("  ── ③ 内核的 GPU 块里不许有同步等待（**假的 async 外壳**）──");
// **∴ 只看**被 gpu feature 关住的那两段**✗（**避免误伤别处 ✓）
const gpuRegion = probeBody + quantBody;
check(!gpuRegion.includes("block_on"), "内核 GPU 块不许出现 block_on（**∴ 那是假修**）");
check(
  !/poll\(\s*wgpu::PollType::Wait\s*\)/.test(gpuRegion),
  "内核 GPU 块不许出现 poll(PollType::Wait)（**∴ wasm 上死锁**）",
);

console.log("  ── ④ Cargo.toml 的 gpu feature 三项齐全 ──");
// **∴ 取数组要用**行首的 `]`**收尾**✗（**第 481 轮实测 ✓）**：
//   **∴ 因为**注释里**可能出现方括号**✗
//     ⇒ **∴ 用**非贪婪的 `[\s\S]*?]`**会**提前闭合** ✓ ★**** ✓✓
const featLine = (toml.match(/^gpu\s*=\s*\[[\s\S]*?^\]/m) || [""])[0];
check(featLine.includes('"yanshi-render/gpu"'), 'gpu feature 必须含 "yanshi-render/gpu"');
check(featLine.includes('"dep:yanshi-gpu"'), 'gpu feature 必须含 "dep:yanshi-gpu"');
check(
  featLine.includes('"dep:wasm-bindgen-futures"'),
  'gpu feature 必须含 "dep:wasm-bindgen-futures"（**#[wasm_bindgen] async fn 的展开要它**）',
);

console.log("  ── ⑤ 默认 feature 不许含 gpu（**体积**）──");
const defLine = (toml.match(/^default\s*=\s*\[[^\]]*\]/m) || [""])[0];
check(!defLine.includes("gpu"), `default feature 不许含 gpu（实测：${defLine.trim() || "缺 default 行"}）`);

console.log("  ── ⑥ 前端真的调它（**否则导出悬空**）──");
// **∴ 为什么 ✗**：**导出了但没人调**✗ ⇒ **∴ 那**等于没接** ✓
//   ⇒ **∴ 所以**：**判据要**同时看**导出端 ＋ 调用端** ✓ ★**** ✓✓
const VIEWER = join(ROOT, "crates/yanshi-http/assets/viewer-app.js");
if (!existsSync(VIEWER)) {
  console.error(`  ✗ 判据自身的输入缺失（不是产品问题）：${VIEWER}`);
  process.exit(3);
}
const viewer = readFileSync(VIEWER, "utf8");
check(/module\.gpu_probe_async/.test(viewer), "viewer-app.js 必须调 module.gpu_probe_async");
check(
  /typeof module\.gpu_probe_async === "function"/.test(viewer),
  "调用前必须查 typeof（**∴ 默认产物**没有它 ⇒ **∴ 不许**直接调 ✓）",
);
check(
  /await module\.gpu_probe_async\(\)/.test(viewer),
  "必须 await 它（**∴ 不许**只拿 Promise 不看结果 ✓）",
);
check(
  /__yanshiKernelGpu/.test(viewer),
  "探测结果要落到 window.__yanshiKernelGpu（**∴ 可被判据读取 ✓）",
);
// **∴ 诚实性 ✗**：**探测**不许**写成「**正在用 GPU**」 ✓
check(
  !/render_backend\s*=\s*["']gpu["']/.test(viewer) && !/webgpu_used_by_kernel\s*[:=]\s*true/.test(viewer),
  "前端不许因为探测成功就**声称**在用 GPU（**∴ 那是谎报 ✓）",
);

console.log("  ── ⑦ 渲染路也接上了（**否则只探测不渲染**）──");
// **∴ 为什么 ✗**：**只探测不渲染**等于**没用上** ✓
//   ⇒ **∴ 必须**有**调用方**真的**走异步渲染** ✓ ★**** ✓✓
check(/async function kernelRenderRgba/.test(viewer), "viewer-app.js 必须有 async function kernelRenderRgba");
check(
  /typeof kernel\.render_region_rgba_async === "function"/.test(viewer),
  "必须查 render_region_rgba_async 是否存在（**∴ 默认产物没有它 ✓）",
);
check(
  /await kernel\.render_region_rgba_async\(/.test(viewer),
  "必须 await render_region_rgba_async（**∴ 不许**只拿 Promise ✓）",
);
check(
  /kernel\.render_region_rgba\(/.test(viewer),
  "必须保留同步回退 kernel.render_region_rgba（**∴ 没有 feature 时要能工作 ✓）",
);
check(
  /await kernelRenderRgba\(/.test(viewer),
  "至少有一个调用点真的 await 了这个包装（**∴ 否则渲染路没接上 ✓）",
);

console.log("");
if (failed > 0) {
  console.error(`  ✗ 内核异步 GPU 路有 ${failed} 项不达标（**∴ 那会让浏览器内核**死锁**或**体积失控**）`);
  process.exit(1);
}
console.log("  ✓ 内核异步 GPU 路：真 async ＋ 真 await ＋ 默认关闭（5 组断言全过）");
process.exit(0);
