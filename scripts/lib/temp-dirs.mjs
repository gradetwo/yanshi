//! **★ 临时目录的**统一清理** ✗ ★**（**第 763 轮 ✓；**从第 53 轮的模板抽出 ✓）
//!
//! **∴ 为什么抽成模块 ✗**（**实测的系统性缺陷 ✓）★**：
//!   **∴ 实测 ✗**：**本仓库有**26 个脚本**用 `mkdtempSync`**✗
//!     ＋ **∴ 而**其中**15 个没有清理**✗ ⇒ **∴ 于是**每跑一次**泄漏一个 root** ✓ ★★
//!       ＋ **∴ 证据 ✗**：**用户定期清理时**实测到** 131 个 `gpuprobe-`／`gfp-` 目录
//!         **（**最老到 10 月 11 日 ✓）—— **∴ 而**那只是**两个脚本**的累积** ✓★★
//!   **∴ 根因 ✗**：**模板是**逐脚本复制的**✗ ⇒ **∴ 新脚本**只能**靠人记得** ✓
//!     ＋ **∴ 所以 ✗**：**抽成模块**✗ ⇒ **∴ 导入即正确** ✓ ★★
//!
//! **∴ 用法（**两行 ✓）✗**：
//!   ```js
//!   import { trackTemp } from "./lib/temp-dirs.mjs";
//!   const root = trackTemp(mkdtempSync(join(tmpdir(), "yanshi-")));
//!   ```
//!   ＋ **∴ `trackTemp` **顺带注册**清理 ＋ **信号处理** ⇒ **∴ 调用方**不用再写** ✓ ★
//!
//! **∴ 它做什么 ✗**：**① 收集路径**✗ ／ **② `exit` 时删三遍**（**∴ 每遍之间同步等 200ms ⇒
//!   **∴ 因为**进程**可能还在写那个目录**✗ ⇒ **∴ 一次 `rmSync` 会失败** ✓）
//!   ＋ **∴ ③ `SIGINT`／`SIGTERM`／`SIGHUP` **走同一条清理** ✓ ★
//!
//! **∴ 判据 ✗**：`crates/yanshi-http/tests/scripts_clean_their_temp_dirs.rs`
//!   （**∴ 任何用 `mkdtempSync` 的 `scripts/*.mjs` **必须**导入本模块** ✓；
//!    **变异：**去掉某个脚本的 import ⇒ **∴ 必红** ✓）★
import { rmSync } from "node:fs";

const __tempPaths = [];

/// **∴ 注册一个临时路径 ⇒ 退出／信号时自动删除 ✓**★
export function trackTemp(path) {
  __tempPaths.push(path);
  return path;
}

/// **∴ 删三遍（**进程可能还在写 ✓）★**
export function cleanupTempDirs() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let left = 0;
    for (const path of __tempPaths) {
      try {
        rmSync(path, { recursive: true, force: true });
      } catch {
        left += 1;
      }
    }
    if (left === 0) return;
    try {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200);
    } catch {
      /* 忽略 */
    }
  }
}

/// **∴ 已跟踪的路径数（**判据／诊断用 ✓）★**
export function trackedTempCount() {
  return __tempPaths.length;
}

// **∴ 只在**首次导入**时注册钩子**✗ ⇒ **∴ 重复导入**不会重复注册** ✓ ★
if (!globalThis.__yanshiTempHooksInstalled) {
  globalThis.__yanshiTempHooksInstalled = true;
  process.on("exit", cleanupTempDirs);
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
    process.on(signal, () => {
      cleanupTempDirs();
      process.exit(1);
    });
  }
}
