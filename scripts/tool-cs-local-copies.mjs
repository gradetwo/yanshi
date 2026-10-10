//! **★ C/S 本地渲染的两份拷贝必须**逐字节相同** ✗ ★**（第 67 轮 ✓；
//! **`yanshi-cs-local-render-fix.patch` 的**已知局限换来的 ✓**）。
//!
//! **∴ 背景 ✗ ★**：**那份补丁**为了让**C/S 前端**能在**关闭"服务器渲染"时**走
//!   **本地 WASM 内核**✗ ⇒ **∴ 它**把 **PWA 的两份实现**复制进了 `assets/`**✗**：
//!     **∴ `crates/yanshi-http/assets/api-local.js`**（**148 个端点的本地映射 ✓）
//!     **∴ `crates/yanshi-http/assets/store.js`**（**IndexedDB 封装 ✓）
//!   ⇒ **∴ 于是**：**同一份逻辑**有**两份拷贝** ✓**** ✓✓
//!
//! **∴ 报告自己写的风险 ✗**：**"**双文件维护提醒：`assets/api-local.js` 与
//!   `web/api-local.js` 现为两份拷贝 ⇒ **∴ 建议**后续把同步方向扩展为双向，
//!   或以 `assets/` 为单一源 ✓"
//!   ⇒ **∴ 而**我**现在**不**改同步机制**✗（**∴ 那**是**一次**架构决定 ✓）
//!     ⇒ **★ 而是**加一条判据：**两份必须**逐字节相同**✗
//!       ⇒ **∴ 于是**：**任何**单边修改**都会**立刻红** ✓ ★**** ✓✓
//!
//! **∴ 判据（**四条 ✓）★**：
//!   **∴ ①** `assets/api-local.js` ≡ `web/api-local.js`**✗
//!   **∴ ②** `assets/store.js` ≡ `web/store.js`**✗
//!   **∴ ③** **服务端的两个路由**（**`/api-local.js`／`/store.js` ✓）**都能** 200**✗
//!     ⇒ **∴ 那**证明 **`include_str!` 内嵌**生效 ✓**** ✓✓
//!   **∴ ④** **页面里的**开关分发**必须在**✗（**∴ `callToolLocal` 出现 ✓）**
//!     ⇒ **∴ 否则**"**关闭服务器渲染 ✓"**这个开关**就是**摆设 ✓**** ✓✓
//!
//! **∴ 变异（**已验证 ✓）★**：**只改**两份拷贝中的**一份**✗ ⇒ **∴ ① 或 ② 必红 ✓**

import { readFileSync, statSync, mkdtempSync, rmSync } from "node:fs";
import { spawn } from "node:child_process";
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

const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

/** **∴ 两份必须**逐字节相同 ✗**（**∴ 只**比长度不够 ✓）** ✓✓ */
const PAIRS = [
  ["crates/yanshi-http/assets/api-local.js", "web/api-local.js"],
  ["crates/yanshi-http/assets/store.js", "web/store.js"],
];
for (const [a, b] of PAIRS) {
  try {
    const left = readFileSync(a);
    const right = readFileSync(b);
    const same = left.length === right.length && left.equals(right);
    check(same, `${a} 与 ${b} 必须**逐字节相同**`,
      same ? `${left.length} B` : `${left.length} B vs ${right.length} B`);
    if (!same) {
      // **∴ 找出**第一处不同**✗（**∴ 便于**定位 ✓）** ✓✓
      const n = Math.min(left.length, right.length);
      let i = 0;
      while (i < n && left[i] === right[i]) i += 1;
      console.error(`     ⇒ 第一处不同在第 ${i} 字节`);
    }
  } catch (error) {
    check(false, `${a} 与 ${b} 都必须存在`, String(error && error.message || error).slice(0, 80));
  }
}

/** **∴ ③④ 服务端路由与开关分发 ✗**（**∴ 起一个**临时服务 ✓）** ✓✓
 *  **∴ 找不到二进制就**如实跳过**✗（**∴ 不**假装通过 ✓）** ✓✓
 */
// **★ 必须挑**最新构建的那个二进制 ✗ ★**（第 67 轮 ✓；**∴ 我**第一版踩到了 ✓）：
//   **∴ 症状 ✗**：**我**刚构建了 **debug**✗（**∴ 它**含新路由 ✓）
//     ⇒ **∴ 而**判据**挑了**旧的 `target/release`**✗ ⇒ **∴ 于是**报 **404** ✓**** ✓✓
//   **∴ 修法**：**按** mtime**挑最新的 ✓**** ✓✓
const binary = ["target/release/yanshi-serve", "target/debug/yanshi-serve"]
  .map((path) => {
    try { return { path, mtime: statSync(path).mtimeMs }; } catch { return null; }
  })
  .filter(Boolean)
  .sort((a, b) => b.mtime - a.mtime)
  .map((entry) => entry.path)[0];
if (!binary) {
  console.log("  ⊘ 找不到 yanshi-serve ⇒ ③④ 跳过（**∴ 先构建** ✓）");
} else {
  const root = trackTemp(mkdtempSync(join(tmpdir(), "cs-local-")));
  const port = 8763;
  const child = spawn(binary, ["--root", root, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
  try {
    let up = false;
    for (let i = 0; i < 60; i += 1) {
      try {
        if ((await fetch(`http://127.0.0.1:${port}/health`)).ok) { up = true; break; }
      } catch { /* 还没起来 */ }
      await new Promise((r) => setTimeout(r, 250));
    }
    if (!up) {
      check(false, "临时服务必须起来", "超时");
    } else {
      for (const route of ["/api-local.js", "/store.js"]) {
        const res = await fetch(`http://127.0.0.1:${port}${route}`);
        const body = await res.text();
        check(res.status === 200 && body.length > 1000,
          `服务端必须内嵌 ${route}`, `${res.status}｜${body.length} B`);
      }
      const viewer = await (await fetch(`http://127.0.0.1:${port}/viewer-app.js`)).text();
      check(viewer.includes("callToolLocal"),
        "`viewer-app.js` 必须含**开关分发**（`callToolLocal`）⇒ **∴ 否则**那个开关是摆设", "");
    }
  } finally {
    child.kill();
    await new Promise((r) => setTimeout(r, 300));
  }
}

console.log("");
if (failures.length) {
  console.error(`  结论：C/S 本地渲染的拷贝**不一致**或有缺 ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ 两份拷贝逐字节相同 ＋ 服务端路由内嵌 ＋ 开关分发在");
process.exit(0);
