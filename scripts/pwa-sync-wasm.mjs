#!/usr/bin/env node
// **★ 把内核产物同步到 PWA 的静态目录 ✓ ★**（第 608 轮 ✓；**部署矩阵 §9 ① ✓**）
//
// **∴ 为什么需要它 ✗**：**`crates/yanshi-wasm/pkg/` 是构建产物 ✗**（**不在 `web/` 里 ✓**），
//   而 **PWA 要求**静态托管**✗** ⇒ **∴ 必须把 `pkg/` 拷到 `web/wasm/` ✓**。
//
// **∴ 用法** ✓：`node scripts/pwa-sync-wasm.mjs`（**先 `cargo build -p yanshi-wasm --target
//   wasm32-unknown-unknown` ＋ `wasm-bindgen` 生成 `pkg/` ✓，**或复用已有的 `pkg/` ✓**）
//
// **∴ 判据** ✓：**拷完后 `web/wasm/yanshi_wasm_bg.wasm` 必须存在且非空 ✗**
//   ⇒ **∴ 否则 `wrangler deploy` 会传一个没有内核的 PWA ✗**（**∴ 那就成了"假装能画"✗**）。
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { join } from "node:path";

const src = "crates/yanshi-wasm/pkg";
const dst = "web/wasm";
const files = ["yanshi_wasm.js", "yanshi_wasm_bg.wasm", "yanshi_wasm.d.ts", "yanshi_wasm_bg.wasm.d.ts"];

if (!existsSync(src)) {
  console.error(`✗ 找不到内核产物 ${src} ⇒ 请先构建 yanshi-wasm（见本脚本头部说明）`);
  process.exit(2);
}
mkdirSync(dst, { recursive: true });
let copied = 0;
for (const f of files) {
  const from = join(src, f);
  if (!existsSync(from)) continue;
  copyFileSync(from, join(dst, f));
  copied += 1;
}
const wasm = join(dst, "yanshi_wasm_bg.wasm");
if (!existsSync(wasm) || statSync(wasm).size === 0) {
  console.error("✗ 同步后 wasm 缺失或为空 ⇒ 该 PWA 会没有内核");
  process.exit(1);
}
console.log(`✓ 已同步 ${copied} 个文件 ⇒ ${dst}（wasm ${(statSync(wasm).size / 1048576).toFixed(2)} MiB ✓）`);
