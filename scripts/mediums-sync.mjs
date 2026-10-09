#!/usr/bin/env node
// **★ 把介质插件产物同步到 `assets/mediums/` ✓ ★**（第 666 轮 ✓；**用户报告 ✓**）
//
// **∴ 为什么需要它 ✗**：**这 6 个 `.wasm` 是**构建产物**✗**（**由 `crates/yanshi-medium-*`
//   编译，再由 `package-release.sh:458-481` 拷入 ✓**）⇒ **∴ 它们**不应在版本库**✗**
//   （**∴ 每次重建都会变 ⇒ **∴ `git status` 永远 dirty ✓**）⇒ **∴ 于是**改由本脚本同步 ✓**。
//
// **∴ 目标名与服务端路径的对应 ✓**（**照 `package-release.sh` ✓**）：
//   `yanshi_medium_oil.wasm`        ⇒ `assets/mediums/oil.wasm` ✓
//   `yanshi_medium_watercolor.wasm` ⇒ `assets/mediums/watercolor.wasm` ✓
//   `yanshi_medium_marker.wasm`     ⇒ `assets/mediums/marker.wasm` ✓
//   `yanshi_medium_pencil.wasm`     ⇒ `assets/mediums/pencil.wasm` ✓
//   `yanshi_medium_pixel.wasm`      ⇒ `assets/mediums/pixel.wasm` ✓
//   `yanshi_medium_example.wasm`    ⇒ `assets/mediums/example-dab.wasm` ✓
//
// **∴ 用法** ✓：`node scripts/mediums-sync.mjs [wasm-release-dir]`
//   （**缺省** `target/wasm32-unknown-unknown/release` ✓**）
//
// **∴ 判据** ✓：**拷完后每个目标必须存在且非空 ✗** ⇒ **∴ 否则**报错并说明怎么构建 ✓**。
import { copyFileSync, existsSync, mkdirSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = process.argv[2] || "target/wasm32-unknown-unknown/release";
const DST = "assets/mediums";
const PAIRS = [
  ["yanshi_medium_oil.wasm", "oil.wasm"],
  ["yanshi_medium_watercolor.wasm", "watercolor.wasm"],
  ["yanshi_medium_marker.wasm", "marker.wasm"],
  ["yanshi_medium_pencil.wasm", "pencil.wasm"],
  ["yanshi_medium_pixel.wasm", "pixel.wasm"],
  ["yanshi_medium_example.wasm", "example-dab.wasm"],
];

if (!existsSync(SRC)) {
  console.error("ERROR: 找不到 " + SRC);
  console.error("  先构建：cargo build --release --target wasm32-unknown-unknown -p yanshi-medium-oil -p yanshi-medium-watercolor -p yanshi-medium-marker -p yanshi-medium-pencil -p yanshi-medium-pixel -p yanshi-medium-example");
  process.exit(2);
}
mkdirSync(DST, { recursive: true });
let n = 0;
const missing = [];
for (const [from, to] of PAIRS) {
  const f = join(SRC, from);
  if (!existsSync(f) || statSync(f).size === 0) { missing.push(from); continue; }
  copyFileSync(f, join(DST, to));
  n += 1;
}
if (missing.length) {
  console.error("ERROR: 缺少构建产物：" + missing.join(" / "));
  process.exit(1);
}
console.log("OK: 已同步 " + n + " 个介质插件 => " + DST);
