#!/usr/bin/env node
// **`smart_sample` 的笔触方向必须**跟着参考图的等明度线**（第 1423 轮；外部 §213）
//
// **⇒ 外部报的缺陷** ✗：三组独立实验（画布差 3.3 倍、`ref_bbox` 三种写法、`samples/size` 三组）
//   ⇒ **笔触全是竖条** ⇒ 他排除参数后判定「方向场只有垂直」✓（判断正确 ✓）
// **⇒ 根因（源码）**：`let horizontal = sw >= sh;` ⇒ **方向只由形状长宽比决定**，
//   **与参考图毫无关系** ✗ ⇒ 高位画布（如 600×1066）必然全竖条 ✓
// **⇒ ⇒ 而 summary 承诺**「自动生成**顺应等明度线**的多尺度笔触…**方向取等明度切向**」✗✗
//   ⇒ **文档对、代码错**（本场第三次：`fill_region.opacity`、掩膜遍时间基、这里 ✓）
//
// **⇒ 断什么**（**同一张高位画布**，只换参考图的**渐变方向** ⇒ 笔触方向必须**翻转**）：
//   ① 纵向渐变（等明度线**横**）⇒ 墨的**纵向**变化 > 横向变化（横条）
//   ② 横向渐变（等明度线**竖**）⇒ **横向**变化 > 纵向变化（竖条）
//   **⇒ ⇒ 只验 ① 会漏**：如果方向被写死成"横"，① 也对 ✓ 但 ② 会红 ⇒ **两条一起才封住两个方向** ✓
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { writeFileSync, mkdtempSync } from "node:fs";
import { deflateSync } from "node:zlib";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-smart-sample-direction-follows-ref.mjs <base>");
  process.exit(3);
}
const dir = trackTemp(mkdtempSync(join(tmpdir(), "ssdir-")));
const stamp = Date.now();
const W = 200;
const H = 400;   // **高位画布** ⇒ 旧代码在这里必然全竖条 ✗

const table = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = table[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};
const writePng = (path, fn) => {
  const raw = Buffer.alloc(H * (1 + W * 4));
  let o = 0;
  for (let y = 0; y < H; y += 1) {
    raw[o] = 0; o += 1;
    for (let x = 0; x < W; x += 1) {
      const [r, g, b] = fn(x, y);
      raw[o] = r; raw[o + 1] = g; raw[o + 2] = b; raw[o + 3] = 255; o += 4;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4); ihdr[8] = 8; ihdr[9] = 6;
  writeFileSync(path, Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0)),
  ]));
};
const vgrad = join(dir, "vgrad.png");
const hgrad = join(dir, "hgrad.png");
writePng(vgrad, (x, y) => (y < H / 2 ? [0, 0, 0] : [255, 255, 255]));   // 纵向渐变 ⇒ 等明度线横
writePng(hgrad, (x, y) => (x < W / 2 ? [0, 0, 0] : [255, 255, 255]));   // 横向渐变 ⇒ 等明度线竖

const post = async (doc, tok, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${tok}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};

// **⇒ 量什么**：**笔触自己的方向**（`get_stroke_log` 的 `first_point`/`last_point`）✓
//   **⇒ 不能量什么** ✗：**墨的方差** —— 我第一次就是这么写的，而它**被参考图自身的梯度污染**
//     （纵向渐变的参考图，墨本来就纵向变化大 ⇒ 与笔触方向无关）⇒ **变异没让判据变红** ⇒ 安慰剂 ✗
//     ⇒ 当场发现并换成结构量 ✓（这正是「判据自己也要被测」的又一次实例）
const measure = async (tag, ref) => {
  const doc = `ssd_${tag}_${stamp}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: W, height: H,
      background: { r: 200, g: 200, b: 200, a: 255 } }),
  })).json();
  if (!made.token) return null;
  await post(doc, made.token, "create_layer", { layer_id: "T", name: "T" });
  const r = await post(doc, made.token, "smart_sample", {
    layer_id: "T", ref, shape: { type: "rect", x: 0, y: 0, w: W, h: H },
    samples: 8, density: 0.5, size: 20,
  });
  if (r?.ok !== true) return null;
  const log = await post(doc, made.token, "get_stroke_log", {});
  const entries = (log.entries || []).filter((e) => e.first_point && e.last_point);
  if (entries.length === 0) return null;
  let horiz = 0;
  let vert = 0;
  for (const e of entries) {
    const dx = Math.abs(e.last_point.x - e.first_point.x);
    const dy = Math.abs(e.last_point.y - e.first_point.y);
    if (dx > dy) horiz += 1;
    else if (dy > dx) vert += 1;
  }
  return { total: entries.length, horiz, vert };
};

const a = await measure("vg", vgrad);
const b = await measure("hg", hgrad);
if (!a || !b) {
  console.error("  ⊘ 不可判：smart_sample 失败（不是本判据要断的东西）");
  process.exit(3);
}
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① 纵向渐变（等明度线横）⇒ 笔触应**多数是横的**
if (a.horiz > a.vert) {
  ok(`① 纵向渐变 ⇒ 横笔占多数（横 ${a.horiz} / 竖 ${a.vert}，共 ${a.total} 笔）`);
} else {
  bad(`① 纵向渐变（等明度线横）⇒ 笔触应横着走 ⇒ 实测横 ${a.horiz} / 竖 ${a.vert}，共 ${a.total} 笔`);
}

// ② 横向渐变（等明度线竖）⇒ 笔触应**多数是竖的**
if (b.vert > b.horiz) {
  ok(`② 横向渐变 ⇒ 竖笔占多数（横 ${b.horiz} / 竖 ${b.vert}，共 ${b.total} 笔）`);
} else {
  bad(`② 横向渐变（等明度线竖）⇒ 笔触应竖着走 ⇒ 实测横 ${b.horiz} / 竖 ${b.vert}，共 ${b.total} 笔`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（方向必须跟着参考图的等明度线）`);
  process.exit(1);
}
console.log("\n  ✓ 笔触方向随参考图的等明度线翻转（不再只看形状长宽比）");
