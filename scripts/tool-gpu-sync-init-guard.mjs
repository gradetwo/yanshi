#!/usr/bin/env node
// **★ `yanshi-gpu` 必须有**平台守卫**：wasm32 上**不许**走同步初始化 ✗ ★**（第 443 轮 ✓）
//
// **∴ 为什么必须有这条 ✗**：**本 crate**用自实现的 `block_on`**✗（**∴ 不引入 `pollster` ✓）
//   ⇒ **∴ 而** `block_on` **在 `wasm32`（**单线程 ＋ 事件循环 ✓）上**永远等不到**
//     **`request_adapter` 的完成**✗
//       ⇒ **∴ 于是**：**它**会**死锁** ✓（**∴ 不是**报错，**∴ 而是**挂住 ✓）
//         ⇒ **★ 所以 ✗ ★**：**必须**先问平台**✗ ⇒ **∴ 不支持**就**明确报错** ✓
//           ⇒ **★ 那**守住了**目标第 7 条**：**不许假装用了 GPU ✗ ＋ **不许静默降级** ✓ ★**** ✓✓
//
// **∴ 判据（三条 ✓）★**：
//   **∴ ①** **`pub const fn supports_sync_init()` **在**模块作用域**（**∴ 不在 `impl` 里 ✓）** ✓
//   **∴ ②** **`Quantizer::new` **开头**有 `if !supports_sync_init() { return Err(…`** ✓
//   **∴ ③** **那个 `Err` 的文案**必须**说明**原因**（**含 `wasm32` 或 `死锁` 字样 ✓）★**** ✓✓
//
// **∴ 变异（**两条 ✓）★**：
//   **∴ 甲**：**删掉 `new` 里的守卫** ⇒ **∴ ② 必红** ✓
//   **∴ 乙**：**把 `supports_sync_init` 移进 `impl`** ⇒ **∴ ① 必红** ✓
//
// 用法：node scripts/tool-gpu-sync-init-guard.mjs
import { readFileSync } from "node:fs";

const SRC = "crates/yanshi-gpu/src/lib.rs";
const source = readFileSync(SRC, "utf8");

let failed = 0;
const check = (ok, name, detail) => {
  if (ok) {
    console.log(`  ✓ ${name}（${detail}）`);
  } else {
    console.error(`  ✗ ${name}（${detail}）`);
    failed += 1;
  }
};

// **∴ ① 定义在**模块作用域****✓（**∴ 即** `impl Quantizer` **之前** ✓）
const defIdx = source.indexOf("pub const fn supports_sync_init() -> bool {");
const implIdx = source.indexOf("impl Quantizer {");
check(
  defIdx !== -1 && implIdx !== -1 && defIdx < implIdx,
  "`supports_sync_init` 必须定义在**模块作用域**（**`impl Quantizer` 之前** ✓）",
  defIdx === -1 ? "找不到定义" : `定义在 ${defIdx}｜impl 在 ${implIdx}`
);

// **∴ ② `new` **开头**有守卫** ✓
const newIdx = source.indexOf("pub fn new(lut: &[f32]) -> Result<Self, String> {");
const body = newIdx === -1 ? "" : source.slice(newIdx, newIdx + 700);
check(
  /if\s*!\s*supports_sync_init\(\)\s*\{\s*return\s+Err\(/.test(body),
  "`Quantizer::new` 必须在**开头**就守住平台（**`if !supports_sync_init() { return Err(…`** ✓）",
  newIdx === -1 ? "找不到 `new`" : "已检查函数体前 700 字节"
);

// **∴ ③ `Err` 文案**说明原因** ✓
check(
  /wasm32|死锁/.test(body),
  "`Err` 文案必须**说明原因**（**含 `wasm32` 或 `死锁` 字样** ✓；**∴ 不许**只写一句「不支持」 ✓）",
  /wasm32/.test(body) ? "含 wasm32" : (/死锁/.test(body) ? "含 死锁" : "两者都没有")
);

console.log("");
if (failed === 0) {
  console.log("  结论：✓ 平台守卫在位（**wasm32 上明确报错 ⇒ 不死锁** ✓）");
  process.exit(0);
} else {
  console.error(`  结论：✗ 平台守卫缺失（${failed} 条）`);
  process.exit(1);
}
