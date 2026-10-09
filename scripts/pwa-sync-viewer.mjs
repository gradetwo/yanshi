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
import { copyFileSync, existsSync, readdirSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
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
// **★ 找**能用的**二进制 ✗ ★**（第 220 轮 ✓；**用户实测报错 ✓**）：
//   **∴ 为什么探测 ✗**：**`target/release/yanshi-serve` **可能是旧的**✗**（**如**用户的那个：
//   **不含 `--export-viewer-html` ✓）⇒ **∴ 于是**它**只丢一句"未知参数"✗，**而**不告诉你怎么办 ✓**
//   ⇒ **∴ 现在**：**逐个候选**真的**试一次导出 ✗** ⇒ **∴ 选**第一个成功的 ✓**
//   ＋ **∴ 尊重 `CARGO_TARGET_DIR` ✗**（**∴ 本仓库开发时常把产物放到 /tmp ✓）**。
// **★ `make release` 的产物在**带 triple 的目录** ✗ ★**（第 221 轮 ✓；**用户实测 ✓**）：
//   **∴ `scripts/package-release.sh` 用 `cargo build --release --target <triple>` ✗**
//   ⇒ **∴ 于是**二进制落在 `target/<triple>/release/yanshi-serve` ✗**
//   ⇒ **∴ 而**它**不在** `target/release/` ⇒ **∴ 所以**只找后者**会拿到**旧残留 ✓**** ✓✓
const tripleDirs = (() => {
  const out = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    if (!existsSync(root)) continue;
    for (const e of readdirSync(root, { withFileTypes: true })) {
      if (e.isDirectory() && /-unknown-|-apple-|-pc-/.test(e.name)) out.push(join(root, e.name, "release"));
    }
  }
  return out;
})();
// **★ `YANSHI_SERVE_BIN` 优先 ✗ ★**：**∴ 用户**显式指定**时**不再探测 ✓**** ✓✓
const targetDirs = [
  process.env.YANSHI_SERVE_BIN ? null : null,
  ...tripleDirs,
  process.env.CARGO_TARGET_DIR ? join(process.env.CARGO_TARGET_DIR, "release") : null,
  process.env.CARGO_TARGET_DIR ? join(process.env.CARGO_TARGET_DIR, "debug") : null,
  "target/release",
  "target/debug",
].filter(Boolean);
if (process.env.YANSHI_SERVE_BIN) targetDirs.unshift(process.env.YANSHI_SERVE_BIN);
const tmp = join(DST, ".index.export.html");
const triedBins = [];
const skippedBins = [];
let bin = null;
for (const dir of targetDirs) {
  // **∴ 候选**可能是目录 ✗（**∴ 拼 `yanshi-serve` ✓）**，**也**可能是**显式指定的文件**✗**（**`YANSHI_SERVE_BIN` ✓）** ✓✓
  const cand = dir.endsWith("yanshi-serve") ? dir : join(dir, "yanshi-serve");
  if (!existsSync(cand)) {
    skippedBins.push(cand);
    continue;
  }
  try {
    execFileSync(cand, ["--export-viewer-html", tmp], { stdio: "pipe" });
    bin = cand;
    break;
  } catch (e) {
    const msg = `${e.stderr ?? ""}${e.stdout ?? ""}${e.message ?? ""}`;
    triedBins.push({
      cand,
      stale: /未知参数|unknown argument/i.test(msg),
      // **∴ 保存**首行错误 ✗**（**∴ 便于**看到"bad CPU type"这类架构问题 ✓）** ✓✓
      why: msg.split("\n").find((x) => x.trim()) ?? "",
    });
  }
}
// **★ 候选诊断 ✗ ★**（第 222 轮 ✓）：**∴ 一律可查 ✗** ——
//   **∴ 平时**只在失败时打印 ✗**；**设 `YANSHI_SERVE_DEBUG=1` 则**总是打印 ✓**
//   **∴ 于是**：**"它选了谁／试过谁／跳过了谁"**一目了然 ✗**（**∴ 不再需要猜 ✓）** ✓✓
if (process.env.YANSHI_SERVE_DEBUG || !bin) {
  console.log(`  候选二进制：共 ${targetDirs.length} 个`);
  for (const s of skippedBins) console.log(`    · 跳过（不存在）${s}`);
  for (const t of triedBins) {
    console.log(`    · 试过${t.stale ? "（旧，不支持 --export-viewer-html）" : "（运行失败）"} ${t.cand}`);
    if (t.why && !t.stale) console.log(`        原因：${t.why.slice(0, 120)}`);
  }
}
if (!bin && triedBins.length > 0) {
  console.error("✗ 找不到能导出静态页的 yanshi-serve");
  for (const t of triedBins) {
    console.error(`   试过 ${t.cand}${t.stale ? "（**不支持 --export-viewer-html ⇒ 是旧的**）" : ""}`);
  }
  if (triedBins.some((t) => t.stale)) {
    console.error("  修复：先重建二进制，再跑本步骤：");
    console.error("      cargo build --release -p yanshi-server");
  }
  process.exit(1);
}
if (!bin) {
  console.warn("  ⚠️ 未找到 yanshi-serve ⇒ 跳过静态页导出 ✗（web/index.html 保持现状 ✓）");
} else {
  console.log(`  静态页导出用的二进制：${bin} ✓`);
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

// **★ 介质插件也要同步 ✗ ★**（第 673 轮 ✓）：**∴ 前端会 `fetch("/mediums/…")` ✗**
//   （`viewer-app.js:2526` 的 `oil`／`watercolor`／`marker` 等 ✓）
//   ⇒ **∴ 不拷 ⇒ **∴ 油画／水彩／马克笔在 PWA 里全 **404 ✗****
//   ⇒ **∴ 而**笔刷早就拷了（199 支 ✓）⇒ **∴ 这是**同类资源漏了一半**✗**。
//   **∴ 实测缺口** ✓：**`web/mediums` 原本是 **0 个文件 ✗****。
// **∴ 插在文件末尾 ✗**（**∴ 无块边界问题 ✓，**且所需 import 都已在顶部 ✓**）。
const MEDIUM_SRC = "assets/mediums";
const MEDIUM_DST = join(DST, "mediums");
if (existsSync(MEDIUM_SRC)) {
  mkdirSync(MEDIUM_DST, { recursive: true });
  let m = 0;
  for (const f of readdirSync(MEDIUM_SRC)) {
    if (!f.endsWith(".wasm")) continue;
    copyFileSync(join(MEDIUM_SRC, f), join(MEDIUM_DST, f));
    m += 1;
  }
  if (m === 0) { console.error("ERROR: 一个介质插件都没同步 => PWA 的油画与水彩会 404"); process.exit(1); }
  console.log("OK: 已同步 " + m + " 个介质插件 => " + MEDIUM_DST);
} else {
  console.warn("WARN: 找不到 assets/mediums => 先跑 node scripts/mediums-sync.mjs");
}