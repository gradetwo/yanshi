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
import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { readdirSync } from "node:fs";
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


// **★ ② 导出并写入静态页 ✗ ★**（第 637 轮 ✓；**部署矩阵 §14.19 ✓**）：
//   **∴ `web/index.html` **必须是服务端生成的那一份**✗**
//   （**∴ 而不是手写第二份 ⇒ 那会分叉 ✓**）⇒ **∴ 本步**调 `--export-viewer-html` 导出 ＋ 拷入 ✗**。
//   **∴ 若二进制不存在 ✗**（**如纯前端开发 ✓**）⇒ **∴ 跳过并**说明原因 ✗**（**不静默 ✓**）。
import { execFileSync } from "node:child_process";
import { renameSync } from "node:fs";
const BINS = ["target/release/yanshi-serve", "target/debug/yanshi-serve"];
const bin = BINS.find((b) => existsSync(b));
if (!bin) {
  console.warn("  ⚠️ 未找到 yanshi-serve ⇒ 跳过静态页导出 ✗（web/index.html 保持现状 ✓）");
} else {
  const tmp = join(DST, ".index.export.html");
  execFileSync(bin, ["--export-viewer-html", tmp], { stdio: "pipe" });
  const size = statSync(tmp).size;
  if (size < 1000) {
    console.error(`✗ 导出的页面只有 ${size} 字节 ⇒ **∴ 疑似失败 ✗**`);
    process.exit(1);
  }
  const html = readFileSync(tmp, "utf8");
  // **★ 注入本地接管脚本 ✗ ★**（第 638 轮 ✓；**部署矩阵 §14.21 ✓**）：
  // **∴ 服务端页面**必然引用 `/api/*`✗**（**它本来是给服务端用的 ✓**）
  // ⇒ **∴ PWA 里必须**用 `api-local.js` 接管它们 ✗** ⇒ **∴ 否则**会打到静态服务器 ⇒ 404 ✓**。
  // **∴ 注入内容是**固定**的 ✗ ⇒ **∴ 判据可以"**除注入外逐字节相同**"✓**。
  const INJECT = join(DST, "pwa-inject.html");
  if (!existsSync(INJECT)) {
    console.error("✗ 缺少注入片段 web/pwa-inject.html ⇒ **∴ PWA 的 /api/* 会 404 ✗**");
    process.exit(1);
  }
  const snippet = readFileSync(INJECT, "utf8");
  if (!html.includes("</body>")) {
    console.error("✗ 导出的页面里没有 </body> ⇒ **∴ 无法注入 ✗**");
    process.exit(1);
  }
  const injected = html.replace("</body>", snippet + "\n</body>");
  writeFileSync(join(DST, "index.html"), injected);
  console.log(`✓ 已注入本地接管脚本 ⇒ web/index.html（注入 ${snippet.length} 字节 ✓）`);
  console.log(`✓ 静态页已由服务端导出 ⇒ web/index.html（${(size / 1024).toFixed(0)} KiB ✓）`);
}

// **★ 笔刷资源（`.myb`）同步 ✓ ★**（第 651 轮 ✓；**部署矩阵 §14.30 ✓**）：
//   **∴ 内核的落笔要的是**笔刷文件全文**✗**（**PaintRequest.myb ✓**）
//   ⇒ **∴ 无服务器部署必须自带这 199 个文件（**≈1.9 MB ✓**）⇒ **∴ 且**9 个文件名含 `#` ✗**
//   ⇒ **∴ 前端编成 %23 ✓ ⇒ **∴ 静态服务器必须正确解码 ✓**（**见 server.rs:486-500 ✓**）。
const BRUSH_SRC = "assets/brushes";
const BRUSH_DST = join(DST, "brushes");
if (!existsSync(BRUSH_SRC)) {
  console.warn("  WARN: 找不到 assets/brushes ⇒ 跳过笔刷同步（笔触在 PWA 里会失败）");
} else {
  mkdirSync(BRUSH_DST, { recursive: true });
  let n = 0;
  for (const f of readdirSync(BRUSH_SRC)) {
    if (!f.endsWith(".myb")) continue;
    copyFileSync(join(BRUSH_SRC, f), join(BRUSH_DST, f));
    n += 1;
  }
  if (n === 0) { console.error("ERROR: 一支笔刷都没同步 ⇒ PWA 无法落笔"); process.exit(1); }
  console.log("OK: 已同步 " + n + " 支笔刷 ⇒ " + BRUSH_DST);
}