#!/usr/bin/env node
// **★ 随仓库发出的可平铺纹理资产（P1 ⑦ 的另一条路 ✓）★**（2026-10-15 ✓）
//
// **为什么有这条判据** ✗：`assets/textures/` 是**运行时缓存**（联网抓取 ✓、**不在 git 里** ✗）
// ⇒ **离线与发布包拿不到纹理** ✗ ⇒ 所以另有一套**随仓库发出**的小瓦片（`assets/tileable/` ✓）。
//   ⇒ 而"随仓库发出"这件事必须**有人守** ✓：文件在不在 ✓、是不是 PNG ✓、大不大 ✓、
//     以及——**最要紧的**——它**还平不平铺** ✓（有人换成一张普通照片，那就白搭 ✗）。
//
// **判据（三条，都能红 ✓）**：
//   ① 至少 2 张 `*.png` ✓，且每张 ≤ 64 KB ✓（随仓库发的东西不能胖 ✓）；
//   ② 每张都是**自写解码器能读的 PNG**（8 位、无隔行、颜色类型 0/2/4/6 ✓）
//      —— 本仓库只解 PNG、且是**自己写的**解码器 ✓ ⇒ 这条是"发出去能不能用"的前提 ✓；
//   ③ **可平铺** ✓：把"左右边缘的平均像素差"和"上下边缘的"与
//      "内部相邻两列的平均差"（＝纹理自身噪声水平 ✓）比 ⇒ **同量级**才算无缝 ✓
//      （阈值：边缘 ≤ max(4, 内部 × 3) ✓ —— 与 `tilecheck` 实测的四张一致 ✓）。
//
// **变异（能红的根据 ✓）**：把任意一张换成 `assets/textures/` 里的**原图**（1024² 照片 ✓）
//   ⇒ ②（体积 > 64 KB ✓）与 ③（边缘差远大于内部 ✓）**都会红** ✓。
//
// 用法：node scripts/tool-tileable-textures.mjs [仓库根]
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { inflateSync } from "node:zlib";

const root = process.argv[2] || ".";
const dir = join(root, "assets", "tileable");

/** 极小 PNG 解码（只支持 8 位、无隔行、颜色类型 0/2/4/6 ✓ —— 与仓库自写解码器同一集合 ✓）。 */
function decodePng(buf) {
  if (buf.subarray(0, 8).toString("binary") !== "\x89PNG\r\n\x1a\n") throw new Error("不是 PNG");
  let pos = 8;
  let idat = [];
  let ihdr = null;
  while (pos + 8 <= buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.subarray(pos + 4, pos + 8).toString("binary");
    const body = buf.subarray(pos + 8, pos + 8 + len);
    if (type === "IHDR") {
      ihdr = {
        w: body.readUInt32BE(0),
        h: body.readUInt32BE(4),
        depth: body[8],
        color: body[9],
        comp: body[10],
        filter: body[11],
        interlace: body[12],
      };
    } else if (type === "IDAT") {
      idat.push(body);
    } else if (type === "IEND") break;
    pos += 12 + len;
  }
  if (!ihdr) throw new Error("缺 IHDR");
  if (ihdr.depth !== 8 || ihdr.interlace !== 0 || ihdr.comp !== 0) {
    throw new Error(`只支持 8 位无隔行（depth=${ihdr.depth} interlace=${ihdr.interlace}）`);
  }
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[ihdr.color];
  if (!channels) throw new Error(`颜色类型 ${ihdr.color} 不支持`);
  const raw = inflateSync(Buffer.concat(idat));
  const stride = ihdr.w * channels;
  const out = Buffer.alloc(ihdr.h * stride);
  let prev = Buffer.alloc(stride);
  let p = 0;
  for (let y = 0; y < ihdr.h; y += 1) {
    const f = raw[p];
    p += 1;
    const line = Buffer.from(raw.subarray(p, p + stride));
    p += stride;
    for (let i = 0; i < stride; i += 1) {
      const a = i >= channels ? line[i - channels] : 0;
      const b = prev[i];
      const c = i >= channels ? prev[i - channels] : 0;
      if (f === 1) line[i] = (line[i] + a) & 0xff;
      else if (f === 2) line[i] = (line[i] + b) & 0xff;
      else if (f === 3) line[i] = (line[i] + ((a + b) >> 1)) & 0xff;
      else if (f === 4) {
        const pa = Math.abs(b - c);
        const pb = Math.abs(a - c);
        const pc = Math.abs(a + b - 2 * c);
        const pr = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
        line[i] = (line[i] + pr) & 0xff;
      } else if (f !== 0) throw new Error(`未知过滤类型 ${f}`);
    }
    line.copy(out, y * stride);
    prev = line;
  }
  return { w: ihdr.w, h: ihdr.h, ch: channels, px: out };
}

function edgeStats(img) {
  const { w, h, ch, px } = img;
  const stride = w * ch;
  const colDiff = (x1, x2) => {
    let tot = 0;
    for (let y = 0; y < h; y += 1) {
      for (let c = 0; c < ch; c += 1) {
        tot += Math.abs(px[y * stride + x1 * ch + c] - px[y * stride + x2 * ch + c]);
      }
    }
    return tot / (h * ch);
  };
  const rowDiff = (y1, y2) => {
    let tot = 0;
    for (let x = 0; x < w; x += 1) {
      for (let c = 0; c < ch; c += 1) {
        tot += Math.abs(px[y1 * stride + x * ch + c] - px[y2 * stride + x * ch + c]);
      }
    }
    return tot / (w * ch);
  };
  return {
    lr: colDiff(0, w - 1),
    tb: rowDiff(0, h - 1),
    inner: Math.max(0.001, colDiff(w >> 1, (w >> 1) + 1)),
  };
}

let files;
try {
  files = readdirSync(dir).filter((f) => f.endsWith(".png"));
} catch (e) {
  console.error(`❌ 判不了：读不到 ${dir}（${e.message}）⇒ 退出码 3`);
  process.exit(3);
}

const fails = [];
if (files.length < 2) fails.push(`① ${dir} 里的 png 少于 2 张（现在 ${files.length} 张）`);
const MAX_BYTES = 64 * 1024;
for (const f of files.sort()) {
  const path = join(dir, f);
  const buf = readFileSync(path);
  if (buf.length > MAX_BYTES) {
    fails.push(`① ${f} 有 ${Math.round(buf.length / 1024)} KB ⇒ 超过 ${MAX_BYTES / 1024} KB ✗（随仓库发的东西不能胖）`);
  }
  let img;
  try {
    img = decodePng(buf);
  } catch (e) {
    fails.push(`② ${f} 不是本仓库解码器能读的 PNG：${e.message}`);
    continue;
  }
  const s = edgeStats(img);
  const limit = Math.max(4, s.inner * 3);
  const ok = s.lr <= limit && s.tb <= limit;
  console.log(
    `  ${f}  ${img.w}×${img.h} ${Math.round(buf.length / 1024)} KB｜左右 ${s.lr.toFixed(2)}｜上下 ${s.tb.toFixed(2)}` +
      `｜内部 ${s.inner.toFixed(2)}｜上限 ${limit.toFixed(2)} ⇒ ${ok ? "可平铺 ✓" : "**不平铺** ✗"}`,
  );
  if (!ok) {
    fails.push(
      `③ ${f} **不平铺** ✗：边缘差（左右 ${s.lr.toFixed(2)}／上下 ${s.tb.toFixed(2)}）远大于内部噪声` +
        `（${s.inner.toFixed(2)}）⇒ 多半是被换成了一张普通照片 ✓`,
    );
  }
}

if (fails.length) {
  console.error("❌ 没通过：");
  for (const f of fails) console.error(`   ${f}`);
  process.exit(1);
}
console.log(`✓ ${files.length} 张可平铺纹理资产都在、都不胖、都能被本仓库解码、且都平铺 ✓`);
