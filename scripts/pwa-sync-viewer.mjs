#!/usr/bin/env node
// **★ 把现有 viewer 的源**同步**到 PWA 静态目录 ✓ ★**（第 632 轮 ✓；**部署矩阵 §12 ✓**）
//
// **★ 为什么是"同步"而不是"复制一份再改" ✗ ★**（**用户要求"不影响现有 WEB" ✓**）：
//   **∴ 单一源**永远是 `crates/yanshi-http/assets/` ✗**（**现有 WEB 在用 ✓**）
//   ⇒ **∴ 本脚本把它**原样拷**到 `web/` ✗** ⇒ **∴ 于是**：
//     **① 两条部署**不会分叉 ✗**（**判据断言两侧逐字节相同 ✓**）；
//     **② `crates/yanshi-http/assets/` **一个字节都不用改 ✓****。
//
// **∴ PWA 侧为什么能跑同一份前端 ✗**：**它调用的 `/api/*` 由 `web/api-local.js` 接管 ✗**
//（**∴ 拦截 `window.fetch` ⇒ 转到本地内核 ＋ IndexedDB ✓**）⇒ **∴ 前端本身**无需分支 ✓****。
//
// **用法** ✓：`node scripts/pwa-sync-viewer.mjs`（**在 `pwa-sync-wasm.mjs` 之后跑 ✓**）
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = "crates/yanshi-http/assets";
const DST = "web";
const FILES = ["viewer-app.js", "viewer.css", "service-worker.js"];

if (!existsSync(SRC)) {
  console.error(`✗ 找不到现有 viewer 的源 ${SRC} ⇒ **∴ 不许**另造一份 ✗**`);
  process.exit(2);
}
mkdirSync(DST, { recursive: true });
let copied = 0;
for (const f of FILES) {
  const from = join(SRC, f);
  if (!existsSync(from)) { console.warn(`  （跳过缺失项 ${f} ✓）`); continue; }
  copyFileSync(from, join(DST, f));
  copied += 1;
}
// **∴ 判据：至少要拷到主前端 ✗**（**否则 PWA 没有界面 ✓**）。
const main = join(DST, "viewer-app.js");
if (!existsSync(main) || statSync(main).size === 0) {
  console.error("✗ 主前端未同步 ⇒ **∴ 该 PWA 没有界面 ✗**");
  process.exit(1);
}
console.log(`✓ 已同步 ${copied} 个文件 ⇒ ${DST}（viewer-app.js ${(statSync(main).size / 1024).toFixed(0)} KiB ✓）`);
