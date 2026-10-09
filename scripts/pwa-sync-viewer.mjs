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
import { rmSync, cpSync, copyFileSync, existsSync, readdirSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const SRC = "crates/yanshi-http/assets";
const DST = "web";
// Rewrite the service worker cache name with a stamp derived from the files that
// ship, so a deployment always gets a fresh cache and the previous one is deleted.
// Without this the fixed name meant stale entries outlived every deploy.
// **★ 緩存戳必須覆蓋所有會影響運行的文件**（定位 ✓）：
//   之前只跟蹤 viewer-app.js/viewer.css/index.html，
//   WASM 重建但 viewer-app.js 未變時，戳不變 ⇒ 瀏覽器繼續用舊 WASM ⇒ MyPaint 筆刷零像素。
const stampSource = [
  "viewer-app.js", "viewer.css", "index.html",
  "api-local.js", "brush-local.js", "store.js",
  "wasm/yanshi_wasm.js", "wasm/yanshi_wasm_bg.wasm",
]
  .map((f) => { try { return statSync(join(DST, f)).mtimeMs; } catch { return 0; } })
  .join("-");
// 用所有文件的 mtime 拼接做戳，而不是只取第一個
const BUILD_STAMP = stampSource.replace(/[^0-9]/g, "").slice(-13) || String(Date.now());
try {
  const swPath = join(DST, "sw.js");
  const swText = readFileSync(swPath, "utf8").replace("__BUILD_STAMP__", BUILD_STAMP);
  writeFileSync(swPath, swText);
  console.log("  OK: service worker cache stamp ⇒ " + BUILD_STAMP);
} catch (e) {
  console.warn("  WARN: 未能写入 service worker 缓存戳：" + e.message);
}

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
// 第 829 轮：注入点必须在最前面（原来插在 </body> 之前 => 哨兵落在页面末尾）。
//   实测：index.html 里哨兵在 10569 行，而第一个脚本在 559 行 => 早期请求全漏过 => 405。
//   现在插到 <head> 之后 => 哨兵真的最先跑；安装块也提前（它是 defer，语义不变）。
const headTag = html.indexOf("<head>");
const injected =
  headTag >= 0
    ? html.slice(0, headTag + 6) + String.fromCharCode(10) + snippet + html.slice(headTag + 6)
    : html.replace("</body>", snippet + String.fromCharCode(10) + "</body>");
if (headTag < 0) {
  console.warn("  WARN: page has no <head> tag, sentinel falls back to before </body>");
}
  writeFileSync(join(DST, "index.html"), injected);
  console.log(`✓ 已注入本地接管脚本 ⇒ web/index.html（注入 ${snippet.length} 字节 ✓）`);
  console.log(`✓ 静态页已由服务端导出 ⇒ web/index.html（${(size / 1024).toFixed(0)} KiB ✓）`);
}

// **★ 笔刷资源（`.myb`）同步 ✓ ★**（第 651 轮 ✓；**部署矩阵 §14.30 ✓**）：
//   **∴ 内核的落笔要的是**笔刷文件全文**✗**（**PaintRequest.myb ✓**）
//   ⇒ **∴ 无服务器部署必须自带这 199 个文件（**≈1.9 MB ✓**）⇒ **∴ 且**9 个文件名含 `#` ✗**
//   ⇒ **∴ 前端编成 %23 ✓ ⇒ **∴ 静态服务器必须正确解码 ✓**（**见 server.rs:486-500 ✓**）。
// **★ 品牌资产也要同步 ✗ ★**（第 829 轮 ✓；**用户报告 `/brand/svg/icon-light.svg` 缺失 ✓**）：
//   **∴ 为什么 ✗**：**导出的页面**引用了 `/brand/svg/icon-light.svg` ✗**
//   ⇒ **∴ 而**同步**原来只拷**笔刷与介质 ✗** ⇒ **∴ 于是**线上**没有 `web/brand/`**✗
//     ⇒ **∴ 它**返回 405 ✓** ⇒ **∴ 图标**显示不出来 ✓**** ✓✓
//   **∴ 现在**：**整棵 `assets/brand/`**递归拷进 `web/brand/` ✗**
//     ⇒ **∴ 于是**网页引用的图标**全都在 ✓**** ✓✓
// **★ 示例资源也要同步 ✗ ★**（第 833 轮 ✓；**静态一致性检查抓到 ✓**）：
//   **∴ 为什么 ✗**：**导出的页面**请求 `/samples/…` ✗** ⇒ **∴ 而**它**不在 `/api/` 下 ✗**
//     ⇒ **∴ 本地层**放行 ✗** ⇒ **∴ 静态托管**找不到 ⇒ **∴ 405 ✓**** ✓✓
//   **∴ 现在**：**整棵 `assets/samples/`**递归拷进 `web/samples/` ✗** ⇒ **∴ 于是**它可访问 ✓**** ✓✓
// Brush preview index and images: the viewer requests /brush-previews/index.json and
// falls back to it offline, but the static host never had the directory (P2-2, user
// report), so the request fell through to the single page fallback and returned HTML.
const BRUSHPREVIEW_SRC = "assets/brush-previews";
const BRUSHPREVIEW_DST = join(DST, "brush-previews");
if (existsSync(BRUSHPREVIEW_SRC)) {
  rmSync(BRUSHPREVIEW_DST, { recursive: true, force: true });
  cpSync(BRUSHPREVIEW_SRC, BRUSHPREVIEW_DST, { recursive: true });
  let pn = 0;
  const pwalk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.isDirectory()) pwalk(join(dir, e.name));
      else pn += 1;
    }
  };
  pwalk(BRUSHPREVIEW_DST);
  console.log("  OK: 已同步笔刷预览 " + pn + " 个 ⇒ " + BRUSHPREVIEW_DST);
} else {
  console.warn("  WARN: 找不到 assets/brush-previews ⇒ 跳过笔刷预览同步");
}

const SAMPLES_SRC = "assets/samples";
const SAMPLES_DST = join(DST, "samples");
if (existsSync(SAMPLES_SRC)) {
  rmSync(SAMPLES_DST, { recursive: true, force: true });
  cpSync(SAMPLES_SRC, SAMPLES_DST, { recursive: true });
  let sn = 0;
  const swalk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.isDirectory()) swalk(join(dir, e.name));
      else sn += 1;
    }
  };
  swalk(SAMPLES_DST);
  console.log("  OK: 已同步示例资源 " + sn + " 个 ⇒ " + SAMPLES_DST);
} else {
  console.warn("  WARN: 找不到 assets/samples ⇒ 跳过示例资源同步");
}

const BRAND_SRC = "assets/brand";
const BRAND_DST = join(DST, "brand");
if (existsSync(BRAND_SRC)) {
  rmSync(BRAND_DST, { recursive: true, force: true });
  cpSync(BRAND_SRC, BRAND_DST, { recursive: true });
  let bn = 0;
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      if (e.isDirectory()) walk(join(dir, e.name));
      else bn += 1;
    }
  };
  walk(BRAND_DST);
  console.log("  OK: 已同步品牌资产 " + bn + " 个 ⇒ " + BRAND_DST);
} else {
  console.warn("  WARN: 找不到 assets/brand ⇒ 跳过品牌资产同步（页面图标会缺）");
}

const BRUSH_SRC = "assets/brushes";
const BRUSH_DST = join(DST, "brushes");
if (!existsSync(BRUSH_SRC)) {
  console.warn("  WARN: 找不到 assets/brushes ⇒ 跳过笔刷同步（笔触在 PWA 里会失败）");
} else {
  mkdirSync(BRUSH_DST, { recursive: true });
  let n = 0;
  for (const f of readdirSync(BRUSH_SRC)) {
    if (!f.endsWith(".myb")) continue;
    // **★ 落盘名用**URI 编码形式** ✗ ★**（第 798 轮 ✓；**用户部署报错 ✓**）：
    //   **∴ 为什么 ✗**：**Cloudflare 的 assets manifest **要求路径是 URI 编码形式 ✗**
    //   ⇒ **∴ 原名（**`#`／`%` ✓）会被**拒收整个上传 ✗****（**错误码 10304 ✓）。
    //   **∴ 而**前端已经**请求编码后的路径 ✗**（**`brush-local.js:28` ✓）⇒ **∴ 两侧一致 ✓**。
        // Percent and hash cannot survive the CDN manifest round trip reliably, so the
// deployed name uses plain ASCII substitutes and the client applies the same map.
const dstName = f.replace(/%/g, "_pct_").replace(/#/g, "_n_");
        copyFileSync(join(BRUSH_SRC, f), join(BRUSH_DST, dstName));
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
// **★ 调色板与纹理也要部署 ＋ 生成索引 ✗ ★**（第 930 轮 ✓；**用户报的 `/api/tools/list_assets` ✓**）：
//   **∴ 为什么 ✗**：**静态托管没有目录列表 ✗** ⇒ **∴ 前端要问"**有哪些资产**"**
//     只能**读一份索引 ✓** ⇒ **∴ 而**这两类资产**从来没进过 `web/` ✓**
//     ⇒ **∴ 于是**：**调色板与纹理面板**永远 501 ✓（**实测 ✓）** ✓✓
//   **∴ 两面（**如实 ✓）★**：**收益**＝**两类资产面板可用 ✗**；
//     **代价**＝**部署体积 ＋13 M ✗**（**纹理 ✓）——
//       **∴ 而**静态托管**按需取 ✗ ⇒ **∴ 运行时不付代价 ✓**
//         （**∴ 只有**真的用到纹理时**才下载 ✓）** ✓✓
const PALETTE_SRC = "assets/palettes";
const PALETTE_DST = join(DST, "palettes");
const TEXTURE_SRC = "assets/textures";
const TEXTURE_DST = join(DST, "textures");
const assetIndex = { palette: [], texture: [], brush: [], medium: [] };
const copyKind = (src, dst, key) => {
  if (!existsSync(src)) { console.warn("  WARN: 找不到 " + src + " => 跳过"); return; }
  rmSync(dst, { recursive: true, force: true });
  cpSync(src, dst, { recursive: true });
  for (const f of readdirSync(dst)) {
    const full = join(dst, f);
    try { if (!statSync(full).isFile()) continue; } catch { continue; }
    assetIndex[key].push({ name: f, bytes: statSync(full).size });
  }
  console.log("  OK: 已同步 " + key + " 资产 " + assetIndex[key].length + " 个 => " + dst);
};
copyKind(PALETTE_SRC, PALETTE_DST, "palette");
copyKind(TEXTURE_SRC, TEXTURE_DST, "texture");
// 笔刷与介质前面已经同步过 => 这里只登记名字（**索引要覆盖前端会问的四类**）。
for (const [src, key] of [[join(DST, "brushes"), "brush"], [join(DST, "mediums"), "medium"]]) {
  if (!existsSync(src)) continue;
  for (const f of readdirSync(src)) {
    const full = join(src, f);
    try { if (!statSync(full).isFile()) continue; } catch { continue; }
    assetIndex[key].push({ name: f, bytes: statSync(full).size });
  }
}
writeFileSync(join(DST, "assets-index.json"), JSON.stringify(assetIndex));
console.log("  OK: 资产索引 => " + join(DST, "assets-index.json") +
  "（palette " + assetIndex.palette.length + "｜texture " + assetIndex.texture.length +
  "｜brush " + assetIndex.brush.length + "｜medium " + assetIndex.medium.length + "）");
