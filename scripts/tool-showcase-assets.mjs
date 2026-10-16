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
//   ④ 每张图都能被**静态服务**按原字节取到 ✓（这条由 `tool-pwa-assets` 那一族覆盖 ✓，这里只查引用 ✓）。
//
// **∴ 能红 ✓**：删掉任一张图、或把它从任一版 README／首页里去掉 ⇒ 立刻红 ✓。
//
// 用法：node scripts/tool-showcase-assets.mjs
import { readFileSync, existsSync, statSync } from "node:fs";

const SHOWCASE = [
  "degas-rehearsal-morning.png",
  "degas-blue-dancers.png",
  "degas-backstage-oil.png",
  "degas-four-dancers-oil.png",
];
const MAX_BYTES = 400 * 1024; // 900px 展示副本的量级 ✓（原图 6–25 MB ✗）
const readers = ["README.md", "README.zh-CN.md"];
const page = "web/index.html";

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
  const path = `web/showcase/${name}`;
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
  // ③ 应用首页也要引用它 ✓
  const html = readFileSync(page, "utf8");
  if (!html.includes(`showcase/${name}`)) bad.push(`${page} 没有引用 showcase/${name}`);
}

if (bad.length) {
  console.error(`❌ 作品展示资产有 ${bad.length} 处问题：`);
  for (const item of bad) console.error(`   · ${item}`);
  console.error("   ⇒ 三处（README×2 ＋ 首页）用的是**同一个相对路径** ⇒ 改名/删图会让它静默变裂图 ✗");
  process.exit(1);
}
console.log(`✓ 四张展示图都在、都是合法 PNG 且 ≤400 KB；两版 README 与首页**都引用了每一张** ✓`);
