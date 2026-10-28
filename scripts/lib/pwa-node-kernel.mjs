#!/usr/bin/env node
// **★ 在 node 里加载**真**wasm 内核 ✓ ★**（给需要像素的 PWA 判据用 ✓）。
//
// **∴ 为什么需要它 ✗**：`api-local.js` 里所有「要像素」的端点都走
//   `__loaders.wasm()`（浏览器读 `/wasm/yanshi_wasm.js` ✓）⇒ **node 里默认路径 import 不到** ✗
//   ⇒ **∴ 于是**这些端点在 node 判据里只能 501 ⇒ **∴ 它们实际上没被验过** ✗。
//   **∴ 内核的 glue 是 `--target web` ✗**（自己 `fetch` 那个 `.wasm` ✓）
//   ⇒ **∴ 但**它同时导出 `initSync({module})` ✓ ⇒ **`readFileSync` 的字节直接喂给它就行** ✓。
//
// **∴ 用法 ✓**：`const { local } = await bootLocalWithRealKernel();` ⇒ 之后 `local(new Request(…))` ✓。
//
// **★ 陈旧守卫 ✗ ★**（与 `scripts/pwa-sync-wasm.mjs` 同一条纪律 ✓）：
//   `web/wasm/` 是**构建产物** ✗ ⇒ 它可能比源码旧 ✓ ⇒ 那么每条像素判据都在**测上一版内核** ✗
//   （**底池第 740–742 轮实测过的**：陈旧内核报「文档 0×0」⇒ 去追一个不存在的缺陷 ✗）。
//   ⇒ 所以这里 `kernelStaleness()` 把「哪些源目录比产物新」**列出来** ✓ ⇒ **判据据此 `exit(3)`** ✓
//   （**前提不成立 ≠ 产品失败 ✓**）。
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const KERNEL_SOURCE_DIRS = [
  "crates/yanshi-wasm/src",
  "crates/yanshi-render/src",
  "crates/yanshi-core/src",
];
const PKG_WASM = "crates/yanshi-wasm/pkg/yanshi_wasm_bg.wasm";
const WEB_GLUE = "web/wasm/yanshi_wasm.js";
const WEB_WASM = "web/wasm/yanshi_wasm_bg.wasm";

function newestRsMtime(dir, newest = 0) {
  let entries = [];
  try { entries = readdirSync(dir, { withFileTypes: true }); } catch (e) { return newest; }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) newest = newestRsMtime(full, newest);
    else if (entry.name.endsWith(".rs")) newest = Math.max(newest, statSync(full).mtimeMs);
  }
  return newest;
}

/** **产物是否旧于源码 ✗** ⇒ 返回 `{ fresh, stale, reason }` ✓（不抛错 ✓）。 */
export function kernelStaleness() {
  let built;
  try {
    built = statSync(PKG_WASM).mtimeMs;
  } catch (e) {
    return { fresh: false, stale: [], reason: `找不到内核产物 ${PKG_WASM} ⇒ 先跑 cargo build -p yanshi-wasm --target wasm32-unknown-unknown ＋ wasm-bindgen` };
  }
  const stale = [];
  for (const dir of KERNEL_SOURCE_DIRS) {
    const newest = newestRsMtime(dir, 0);
    if (newest > built) {
      stale.push(`${dir}（源 ${new Date(newest).toISOString()} > 产物 ${new Date(built).toISOString()}）`);
    }
  }
  for (const file of [WEB_GLUE, WEB_WASM]) {
    try {
      if (statSync(file).size === 0) stale.push(`${file}（空文件）`);
    } catch (e) {
      stale.push(`${file}（缺失）`);
    }
  }
  return stale.length
    ? { fresh: false, stale, reason: "内核产物比源码旧（或 web/wasm 缺东西）⇒ 判据会测到上一版内核" }
    : { fresh: true, stale, reason: "" };
}

let cachedModule = null;

/** **加载真内核模块 ✓**（`initSync` 只做一次 ✓）。 */
export async function realKernelModule() {
  if (cachedModule) return cachedModule;
  const glue = await import(pathToFileURL(process.cwd() + "/" + WEB_GLUE).href);
  if (typeof glue.initSync !== "function") {
    throw new Error("web/wasm/yanshi_wasm.js 没有 initSync ⇒ 这份 glue 不是 --target web 的产物？");
  }
  const bytes = readFileSync(process.cwd() + "/" + WEB_WASM);
  if (bytes.length === 0) throw new Error(`${WEB_WASM} 是空文件`);
  glue.initSync({ module: bytes });
  cachedModule = glue;
  return glue;
}

/** **装配 ✓**：真内核算子注入 `__setLocalApiLoadersForTest` ⇒ 返回 `{ local, kernel }` ✓。 */
export async function bootLocalWithRealKernel({ origin = "https://yanshi-local.test" } = {}) {
  const kernel = await realKernelModule();
  const apiUrl = pathToFileURL(process.cwd() + "/web/api-local.js").href;
  const api = await import(apiUrl);
  if (typeof api.__setLocalApiLoadersForTest !== "function") {
    throw new Error("web/api-local.js 没有 __setLocalApiLoadersForTest ⇒ 注入点没了");
  }
  api.__setLocalApiLoadersForTest({ wasm: async () => kernel });
  const stubUrl = pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-stub.mjs").href;
  const { bootPwaLocalApi } = await import(stubUrl);
  const local = await bootPwaLocalApi({ origin, localOnly: true });
  return { local, kernel };
}
