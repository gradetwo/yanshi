#!/usr/bin/env node
// **本地 `resample` 必须按 mime 分流，不许把裸 RGBA 当 PNG**（第 742 轮；外部审计 S-11）
//
// **⇒ 缺陷**：本仓位图是 `image/x-yanshi-raw`（**裸 RGBA** ✓），而本地层**硬编码**
//   `createImageBitmap(new Blob([...], { type: "image/png" }))` ✗ ⇒ 解析失败 ✓
//   ⇒ **对自家位图永远不工作** ✓，且错误被包成 501 ✓（看起来像"内核不支持"✓，最误导 ✓）。
//
// **⇒ 判据（结构检查，两条）**：
//  ① 有 `image/x-yanshi-raw` 的分支 ✓；
//  ② 那条 `createImageBitmap(...image/png)` 的**硬编码**不见了 ✓（改成按 `mime` 传 ✓）。
// **⇒ 变异**：把分支删掉、回到硬编码 png ⇒ 红 ✓。
//
// **⇒ 为什么不是运行时判据** ✗：这段要真 `canvas`／`ImageData`（浏览器）✓，
//   本机这批判据没有 chromium ✓ ⇒ 用源码比对，并说清它只证明"分流在" ✓。
import { readFileSync } from "node:fs";

const src = readFileSync("crates/yanshi-http/assets/api-local.js", "utf8");
const at = src.indexOf('if (path === "/api/tools/resample")');
const end = src.indexOf("// **P2c-59", at);
if (at < 0 || end < 0) {
  console.error("  ⊘ 不可判：找不到本地 `resample` 分支（函数改名了？）");
  process.exit(3);
}
// **先去掉注释** ✓：注释里写着 `image/x-yanshi-raw` 也会让检查通过 ✗
//   —— 我第一次的变异就是这么假绿的（把条件改成 `if (false)`，注释里的字面量还在 ✓）。
const block = src
  .slice(at, end)
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/^\s*\/\/.*$/gm, "");

const checks = [
  {
    what: "有 `image/x-yanshi-raw` 裸 RGBA 分支（在**比较**里，不只是注释）",
    ok: /===\s*"image\/x-yanshi-raw"/.test(block),
  },
  { what: "裸 RGBA 走 `ImageData`（不是 `createImageBitmap`）", ok: /g\.ImageData\(/.test(block) },
  {
    what: "不再把自家位图硬编码成 png",
    ok: !/createImageBitmap\(new Blob\(\[new Uint8Array\(blobBytes\)\], \{ type: "image\/png" \}\)\)/.test(block),
  },
];
for (const check of checks) console.log(`  ${check.ok ? "✓" : "✗"} ${check.what}`);
const missing = checks.filter((check) => !check.ok);
if (missing.length > 0) {
  console.error(`  ✗ ${missing.length} 处缺失 ⇒ 本地重采样对自家裸 RGBA 位图不工作（外部审计 S-11）`);
  process.exit(1);
}
console.log("✓ 本地 resample 按 mime 分流（结构检查：只证明分流在，不声称验证运行时）");
