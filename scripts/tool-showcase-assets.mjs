#!/usr/bin/env node
// **★ 作品展示资产不许烂掉 ✓ ★**（2026-10-16 ✓）
//
// **∴ 为什么要有它 ✗**：四幅德加习作被放进了 **README（中英两版）** 与 **应用首页** ✓ ——
//   三处都靠**同一个相对路径** `web/showcase/<name>.png` ✓ ⇒ 只要有人**改名/删图/挪目录** ✗
//   就会**静默**变成裂图 ✗（README 与应用都不会报错 ✓）⇒ **∴ 这里把它钉住** ✓。
//
// **∴ 判什么（四条 ✓）**：
//   ① 四张图**都在**、都是**合法 PNG**（自己解析签名 ＋ IHDR ✓，不引依赖 ✗）、且**不是巨图** ✓
//      （放的是 900px 的展示副本 ⇒ 单张 ≤ 400 KB ✓，原图 6–25 MB ✗ 不进仓库 ✓）；
//   ② **两版 README** 都引用了**每一张** ✓；
//   ③ **应用首页**（`web/index.html` ✓）也引用了**每一张** ✓；
//   ④ **真的去服务端 fetch 这四个 URL** ✓（`/showcase/<name>` ⇒ 200 ＋ 字节数与本地一致 ✓）。
//
// **∴ 能红 ✓**：删掉任一张图、或把它从任一版 README／首页里去掉 ⇒ 立刻红 ✓。
//
// 用法：node scripts/tool-showcase-assets.mjs [base-url]
import { readFileSync, existsSync, statSync } from "node:fs";

const SHOWCASE = [
  "degas-backstage-oil.png",
  "degas-four-dancers-oil.png",
];
const MAX_BYTES = 400 * 1024; // 900px 展示副本的量级 ✓（原图 6–25 MB ✗）
// **∴ 图住在哪 ✗**：**单一源是 `assets/showcase/`** ✓（服务端也在发它 ✓，见 `server.rs::showcase_asset` ✓）；
//   `web/showcase/` 是**导出产物** ✓（`pwa-sync-viewer.mjs` 从源拷过去 ✓、且已在 `.gitignore` 里 ✓）。
const SOURCE_DIR = "assets/showcase";
const WEB_DIR = "web/showcase";
const readers = ["README.md", "README.zh-CN.md"];
// **★ 页面要查**两个地方** ✗ ★**（2026-10-16 ✓；**CI 教会我的** ✓）：
//   **∴ 症状 ✗**：我第一版只查 `web/index.html` ✓ ⇒ **本机绿、CI 红** ✗，而**不是**判据乱报 ✓ ——
//     CI 会**重新生成** `web/index.html`（`yanshi-serve --export-viewer-html` ＋ 注入 ✓，
//     见 `scripts/pwa-sync-viewer.mjs` ✓）⇒ **∴ 手改那一份会被覆盖** ✗ ⇒ 我的块没了 ✓ ★
//   **∴ 真相 ✗**：页面的**单一源**是 `crates/yanshi-http/src/viewer.rs` ✓（服务端渲的就是它 ✓）；
//     `web/index.html` 只是**导出产物** ✓（但它**故意入库** ✓ —— 缺了它 `wrangler deploy` 会 404 ✓，
//     见 `.gitignore:70` ✓）。
//   ⇒ **∴ 两处都查** ✓：源里必须有 ✓（否则下次导出就丢 ✗）；产物若在，也必须有 ✓。
const sources = [
  ["crates/yanshi-http/src/viewer.rs", "showcase/"],   // 单一源 ✓（服务端页面）
  ["web/index.html", "showcase/"],                      // 导出产物 ✓（故意入库的那一份）
];

/// 只读 PNG 头 ✓（签名 ＋ IHDR 的宽高 ✓）—— **不引依赖** ✓，与 `tool-tileable-textures` 同一手法 ✓。
const pngInfo = (path) => {
  const bytes = readFileSync(path);
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  for (let i = 0; i < signature.length; i += 1) if (bytes[i] !== signature[i]) return null;
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  return { width, height, bytes: bytes.length };
};

const bad = [];
for (const name of SHOWCASE) {
  const path = `${SOURCE_DIR}/${name}`;
  if (!existsSync(path)) {
    bad.push(`${path} 不存在`);
    continue;
  }
  const size = statSync(path).size;
  const info = pngInfo(path);
  if (!info) {
    bad.push(`${path} 不是合法 PNG（签名不对）`);
    continue;
  }
  if (info.width < 600 || info.height < 600) {
    bad.push(`${path} 只有 ${info.width}×${info.height} ⇒ 展示副本不该这么小`);
  }
  if (size > MAX_BYTES) {
    bad.push(`${path} 有 ${(size / 1024).toFixed(0)} KB > ${MAX_BYTES / 1024} KB ⇒ 展示副本不该这么大`);
  }
  console.log(`  ${name}：${info.width}×${info.height}｜${(size / 1024).toFixed(0)} KB ✓`);
  // ② 两版 README 都要引用它 ✓
  for (const reader of readers) {
    if (!existsSync(reader)) {
      bad.push(`${reader} 不存在`);
      continue;
    }
    const text = readFileSync(reader, "utf8");
    if (!text.includes(`web/showcase/${name}`)) bad.push(`${reader} 没有引用 ${path}`);
  }
  // ③ **源**与**导出产物**都要引用它 ✓（只查产物 = 会被 CI 的重新导出打脸 ✗；只查源 = 产物可能没跟上 ✗）
  for (const [file, prefix] of sources) {
    if (!existsSync(file)) {
      bad.push(`${file} 不存在`);
      continue;
    }
    if (!readFileSync(file, "utf8").includes(`${prefix}${name}`)) {
      bad.push(`${file} 没有引用 ${prefix}${name}`);
    }
  }
}

// **★ ④ 真的去服务端取一次 ✗ ★**（2026-10-16 ✓；**这一条是补上我自己的漏** ✓）：
//   **∴ 我漏了什么 ✗**：第一版只查"三处引用了没有" ✓ ⇒ **本机全绿** ✓ 而**服务端首页里四张图全 404** ✗
//     —— 因为图只放进了**导出目录** `web/` ✗，而**服务端发的是它自己的资产树** ✓（`assets/` ✓）。
//   ⇒ **∴ 判据必须**两条腿走路**✗**：**文件在不在** ✓ ＋ **URL 取不取得到** ✓（两本账 ✓）。
const base = (process.argv[2] || "").replace(/\/+$/, "");
if (base) {
  for (const name of SHOWCASE) {
    let status = 0;
    let bytes = 0;
    try {
      const reply = await fetch(`${base}/showcase/${name}`);
      status = reply.status;
      bytes = (await reply.arrayBuffer()).byteLength;
    } catch (error) {
      bad.push(`取不到 ${base}/showcase/${name}（${error.message}）`);
      continue;
    }
    const local = statSync(`${SOURCE_DIR}/${name}`).size;
    console.log(`  GET /showcase/${name} ⇒ HTTP ${status}｜${bytes} 字节（本地 ${local} ✓）`);
    if (status !== 200) bad.push(`服务端发不出 ${name}（HTTP ${status} ✗）⇒ 页面会是裂图 ✗`);
    else if (bytes !== local) bad.push(`${name} 服务端发的字节数 ${bytes} ≠ 本地 ${local} ✗`);
  }
} else {
  console.log("  （没给 base-url ⇒ 跳过第 ④ 条 HTTP 自检 ✓；CI 里会给 ✓）");
}

if (bad.length) {
  console.error(`❌ 作品展示资产有 ${bad.length} 处问题：`);
  for (const item of bad) console.error(`   · ${item}`);
  console.error("   ⇒ 三处（README×2 ＋ 首页）用的是**同一个相对路径** ⇒ 改名/删图会让它静默变裂图 ✗");
  process.exit(1);
}
console.log(`✓ 四张展示图都在、都是合法 PNG 且 ≤400 KB；两版 README 与首页**都引用了每一张** ✓`);
