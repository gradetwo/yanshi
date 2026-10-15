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
//   ③ **可平铺** ✓ —— 用**接缝的二阶差**比（2026-10-15 改严 ✓）：
//      **为什么不用一阶差** ✗：一阶差（相邻两列的绝对差之和 ✓）会被**很均匀的**纹理骗过去 ✓
//      —— 实测：把一张平滑纸的中心**裁剪**出 128² ✓（**根本不平铺** ✗），
//      它的一阶边缘差仍在"内部 ×3"以内 ⇒ **判据没红** ✗ ⇒ 那样等于没牙 ✗。
//      **二阶差** ✓ 看的是**曲率**：接缝处
//        `|2·p(0,c) − p(w−1,c) − p(1,c)|`（列 0 的左右邻居是 w−1 与 1 ✓）
//        与内部的 `|2·p(x,c) − p(x−1,c) − p(x+1,c)|` 比 ✓
//      ⇒ **平滑但不能平铺**的图在接缝处一定有"折角" ✗ ⇒ 红 ✓。
//      **阈值**：接缝 ≤ max(6, 内部 × 4) ✓。
//
// **变异（能红的根据 ✓）**：把任意一张换成 `assets/textures/` 里的**原图**（1024² 照片 ✓）
//   ⇒ ②（体积 > 64 KB ✓）与 ③（边缘差远大于内部 ✓）**都会红** ✓。
//
// 用法：node scripts/tool-tileable-textures.mjs [忽略的参数，例如 harness 传的 base URL] ✓
//   **仓库根从判据自身的位置推** ✓ ⇒ 传什么进来都不影响 ✓。
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

// **★ 仓库根：从**判据自身的位置**推 ✓（2026-10-15 修 ✓）★**
//   **为什么必须这样** ✗：CI 的 harness 会按统一约定传**第一个参数 = base URL** ✓
//   （`scripts/run-criteria.sh` ✓），而我第一版把第一个参数当**仓库根** ✗
//   ⇒ 于是它拿到 `http://127.0.0.1:7884` 去拼路径 ✓ ⇒ `ENOENT` ⇒ **退出 3**
//   ⇒ ⇒ **在 CI 上它永远"跑不了"** ✗ —— 是我本机那次回归扫抓到它的 ✓。
//   **做法** ✓（照 `tool-release-launcher.mjs` 的先例 ✓：**用它自己文件的位置**解析 ✓，
//   这样从任何 cwd 跑都对 ✓，也不受 harness 传参约定影响 ✓）。
const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
// **★ 位置（2026-10-15 改 ✓）★**：这 4 张**搬进了 `assets/textures/`** ✓ ——
//   因为引擎的 `list_textures` **只列那个目录 ＋ 工作区缓存** ✓
//   ⇒ 放在别的目录里**引擎看不见** ✗（那它们就不是"可用纹理"了 ✗）。
const dir = join(root, "assets", "textures");
const WANTED = [
  "TilePaperFine128.png",
  "TilePaperFiber128.png",
  "TileCardboard128.png",
  "TileCanvas128.png",
];

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

/** **二阶差**（曲率）统计 ✓ —— 见文件头 ③ 的说明（一阶差会被均匀纹理骗过去 ✗）。 */
function edgeStats(img) {
  const { w, h, ch, px } = img;
  const stride = w * ch;
  const at = (x, y, c) => px[y * stride + x * ch + c];
  // 内部：所有列（去掉两端 ✓）的列向二阶差平均值 ✓
  let innerTot = 0;
  let innerN = 0;
  for (let x = 1; x < w - 1; x += 1) {
    for (let y = 0; y < h; y += 1) {
      for (let c = 0; c < ch; c += 1) {
        innerTot += Math.abs(2 * at(x, y, c) - at(x - 1, y, c) - at(x + 1, y, c));
        innerN += 1;
      }
    }
  }
  const innerX = innerN ? innerTot / innerN : 0.001;
  // 接缝：列 0 的左右邻居是 w-1 与 1 ✓；列 w-1 的左右邻居是 w-2 与 0 ✓
  let seamX = 0;
  let seamXN = 0;
  for (let y = 0; y < h; y += 1) {
    for (let c = 0; c < ch; c += 1) {
      seamX += Math.abs(2 * at(0, y, c) - at(w - 1, y, c) - at(1, y, c));
      seamX += Math.abs(2 * at(w - 1, y, c) - at(w - 2, y, c) - at(0, y, c));
      seamXN += 2;
    }
  }
  const lr = seamXN ? seamX / seamXN : 0;
  // 同理按行 ✓
  let innerY = 0;
  let innerYN = 0;
  for (let y = 1; y < h - 1; y += 1) {
    for (let x = 0; x < w; x += 1) {
      for (let c = 0; c < ch; c += 1) {
        innerY += Math.abs(2 * at(x, y, c) - at(x, y - 1, c) - at(x, y + 1, c));
        innerYN += 1;
      }
    }
  }
  const innerYv = innerYN ? innerY / innerYN : 0.001;
  let seamY = 0;
  let seamYN = 0;
  for (let x = 0; x < w; x += 1) {
    for (let c = 0; c < ch; c += 1) {
      seamY += Math.abs(2 * at(x, 0, c) - at(x, h - 1, c) - at(x, 1, c));
      seamY += Math.abs(2 * at(x, h - 1, c) - at(x, h - 2, c) - at(x, 0, c));
      seamYN += 2;
    }
  }
  const tb = seamYN ? seamY / seamYN : 0;
  return { lr, tb, inner: Math.max(0.001, Math.max(innerX, innerYv)) };
}

let present;
try {
  const all = new Set(readdirSync(dir));
  present = WANTED.filter((f) => all.has(f));
} catch (e) {
  console.error(`❌ 判不了：读不到 ${dir}（${e.message}）⇒ 退出码 3`);
  process.exit(3);
}

const fails = [];
// **★ 只守这 4 张具名瓦片** ✓：同目录里还有 ambientCG 的 1024² 大图（0.6–2 MB ✓）
//   ⇒ 不能拿 64 KB 的限制去套它们 ✗（那不是这条判据的对象 ✓）。
const missing = WANTED.filter((f) => !present.includes(f));
if (missing.length) fails.push(`① 缺少瓦片：${missing.join("、")} ⇒ 引擎里就少了这几张可用纹理 ✗`);
const files = present;
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
  const limit = Math.max(6, s.inner * 4);
  const ok = s.lr <= limit && s.tb <= limit;
  console.log(
    `  ${f}  ${img.w}×${img.h} ${Math.round(buf.length / 1024)} KB｜左右 ${s.lr.toFixed(2)}｜上下 ${s.tb.toFixed(2)}` +
      `｜内部 ${s.inner.toFixed(2)}｜上限 ${limit.toFixed(2)} ⇒ ${ok ? "可平铺 ✓" : "**不平铺** ✗"}`,
  );
  if (!ok) {
    fails.push(
      `③ ${f} **不平铺** ✗：接缝的**二阶差**（左右 ${s.lr.toFixed(2)}／上下 ${s.tb.toFixed(2)}）` +
        `远大于内部（${s.inner.toFixed(2)}）⇒ 接缝处有"折角" ✓（多半是裁剪/普通照片 ✓）`,
    );
  }
}

if (fails.length) {
  console.error("❌ 没通过：");
  for (const f of fails) console.error(`   ${f}`);
  process.exit(1);
}
console.log(`✓ ${files.length} 张可平铺纹理资产都在、都不胖、都能被本仓库解码、且都平铺 ✓`);
