#!/usr/bin/env node
// **★ 笔刷库缩略图必须是**真 PNG** ✗ ★**（2026-11-05 实测 ✓）
//
// **∴ 为什么需要这条判据** ✗：`generate-brush-previews.mjs` 的回退路径曾经把
//   对象的 **`image/x-yanshi-raw`（RGBA 裸字节）**直接写成 `<name>.myb.png` ✓
//   ⇒ 浏览器里显示 **30/185 个破图** ✓（`file` 说 `data`，开头 64 字节全 0 ✓）
//   ⇒ 而生成脚本照样打印「生成 185 张 ✓」✗ —— **失败没有信号** ✓（本仓唯一的对手 ✓）。
//
// **∴ 判据（五条，都能红 ✓）**：
//   ① `index.json` 在 ✓（不在 ⇒ 前提不成立 ⇒ exit 3，不冒充产品失败 ✓）
//   ② 每个 `files` 映射到的文件都存在 ✓
//   ③ 每个文件都是 **PNG**（magic `89 50 4E 47 0D 0A 1A 0A` ✓）
//   ④ 每个 PNG 的 IHDR 宽高都 > 0 ✓（只认头 24 字节 ✓，不引依赖 ✓）
//   ⑤ 目录里没有**没登记**的 `.png` ✓（文件名与 index 必须一致 ✓；多了 ⇒ 有孤儿 ✓）
//
// **变异**：把任意一张的前 8 字节清零 ⇒ ③ 红；删一张 ⇒ ② 红；多放一张没登记的 ⇒ ⑤ 红。
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const DIR = "assets/brush-previews";
const INDEX = join(DIR, "index.json");
const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const problems = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) problems.push(label);
};

if (!existsSync(INDEX)) {
  console.error(`⊘ 不可判：${INDEX} 不存在（先跑 node scripts/generate-brush-previews.mjs <base>）`);
  process.exit(3);
}
const index = JSON.parse(readFileSync(INDEX, "utf8"));
const files = index.files && typeof index.files === "object" ? index.files : null;
if (!files) {
  console.error(`⊘ 不可判：${INDEX} 里没有 files 映射`);
  process.exit(3);
}
const names = Object.keys(files);
check(names.length > 0, "index.files 非空", `${names.length} 支笔刷`);

const missing = [];
const notPng = [];
const zeroSize = [];
for (const brush of names) {
  const rel = files[brush];
  const path = join(DIR, rel);
  if (!existsSync(path)) { missing.push(rel); continue; }
  const buf = readFileSync(path);
  if (buf.length < 24 || !buf.subarray(0, 8).equals(PNG_MAGIC)) { notPng.push(`${rel}（${buf.length} 字节）`); continue; }
  // IHDR 紧跟 8 字节签名 + 4 字节长度 + 4 字节类型
  const width = buf.readUInt32BE(16);
  const height = buf.readUInt32BE(20);
  if (!(width > 0 && height > 0)) zeroSize.push(`${rel}（${width}×${height}）`);
}

check(missing.length === 0, "每支笔刷的缩略图文件都存在", missing.length ? `缺 ${missing.length}：${missing.slice(0, 3).join(", ")}` : "全部命中");
check(notPng.length === 0, "每张缩略图都是 PNG（magic 正确）", notPng.length ? `${notPng.length} 张不是：${notPng.slice(0, 3).join(", ")}` : `${names.length} 张`);
check(zeroSize.length === 0, "每张 PNG 的宽高都 > 0", zeroSize.length ? zeroSize.slice(0, 3).join(", ") : "全部有效");

const onDisk = readdirSync(DIR).filter((f) => f.endsWith(".png"));
const registered = new Set(Object.values(files));
const orphans = onDisk.filter((f) => !registered.has(f));
check(orphans.length === 0, "目录里没有未登记的 .png（孤儿）", orphans.length ? `${orphans.length}：${orphans.slice(0, 3).join(", ")}` : `${onDisk.length} 张全部登记`);

if (problems.length) {
  console.log(`  ✗ ${problems.length} 项没成立（笔刷库缩略图）`);
  process.exit(1);
}
console.log("  ✓ 笔刷库缩略图：全部是真 PNG、宽高有效、与 index.json 一一对应");
process.exit(0);
