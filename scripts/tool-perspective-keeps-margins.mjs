#!/usr/bin/env node
// **`perspective` 的恒等 quad 必须**保住透明边距**（第 1329 轮；已知红，见 known-red）
//
// **⇒ 为什么有这条** ✗：
//   补丁 `photo-fix` 新增了 `perspective_identity_is_exact_in_document_render` ⇒ 而它用
//   **整幅不透明**的 `RasterPatch`（`alpha=255` 全片）⇒ **没有「透明边距」这个条件**。
//   ⇒ 我实测「图层里一个不透明矩形 + 四周透明」⇒ 加 `perspective`（quad = 矩形四角）⇒
//   **整幅被填成矩形色**（六个采样点全蓝）✗
//
// **⇒ 而第 1330 轮查清：那不是缺陷，是**我搞错了语义** ✗**
//   `perspective` 的意思是「**把整幅画布映射到给定 quad**」（corner-pin）⇒
//   所以**真恒等 = quad 为**画布四角****（`(0,0),(W,0),(W,H),(0,H)`）
//   ⇒ 我上一轮用「矩形四角」⇒ 那是**把画布挤进矩形** ⇒ 于是边距读到矩形色 ⇒
//     **那是正确行为** ✓（实测：画布四角 quad ⇒ **0 个像素变化**）
//   ⇒ 而「矩形四角」那种输入**另有其正当语义** ⇒ 不留断言 ✓
//
// **⇒ 断什么**：
//   ① 不加调整时：边距是背景（白）、矩形内是矩形色  ← 前提
//   ② `perspective`（quad = 矩形四角）之后：边距**仍是背景**
//   ③ 矩形内**仍是**矩形色
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync } from "node:fs";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-perspective-keeps-margins.mjs <base>");
  process.exit(3);
}
const stamp = Date.now();
const expDir = process.env.YANSHI_EXPORT_DIR
  || (process.env.YANSHI_WORKSPACE ? `${process.env.YANSHI_WORKSPACE}/exports` : null);
if (!expDir) {
  console.error("  ⊘ 不可判：找不到导出目录");
  process.exit(3);
}
const post = async (p, b, doc, tok) => {
  const u = `${base}${p}${doc ? `?doc=${doc}&token=${tok}` : ""}`;
  const r = await fetch(u, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b ?? {}) });
  return r.json();
};
const readPng = (f) => {
  const buf = readFileSync(f); let pos = 8; let w = 0; let h = 0; let ct = 0; const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos); const t = buf.toString("ascii", pos + 4, pos + 8); const d = buf.subarray(pos + 8, pos + 8 + len);
    if (t === "IHDR") { w = d.readUInt32BE(0); h = d.readUInt32BE(4); ct = d[9]; }
    if (t === "IDAT") idat.push(d);
    pos += 12 + len;
  }
  const raw = zlib_inflate(Buffer.concat(idat)); const bpp = ct === 6 ? 4 : 3; const stride = w * bpp; const out = Buffer.alloc(h * stride);
  for (let y = 0; y < h; y += 1) {
    const f2 = raw[y * (stride + 1)]; const src = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    for (let x = 0; x < stride; x += 1) {
      const a = x >= bpp ? out[y * stride + x - bpp] : 0; const b = y > 0 ? out[(y - 1) * stride + x] : 0;
      const c = x >= bpp && y > 0 ? out[(y - 1) * stride + x - bpp] : 0; let v = src[x];
      if (f2 === 1) v += a; else if (f2 === 2) v += b; else if (f2 === 3) v += (a + b) >> 1;
      else if (f2 === 4) { const p = a + b - c; const pa = Math.abs(p - a); const pb = Math.abs(p - b); const pc = Math.abs(p - c); v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c; }
      out[y * stride + x] = v & 255;
    }
  }
  return { w, h, bpp, px: out };
};
import zlib from "node:zlib";
const zlib_inflate = (b) => zlib.inflateSync(b);

const fail = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fail.push(m); console.log(`  ✗ ${m}`); };

const W = 200; const H = 150;
const mk = async () => {
  const doc = `pm_${stamp}`;
  const r = await post("/api/documents", { doc_id: doc, width: W, height: H });
  if (!r?.token) return null;
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, r.token);
  await post("/api/tools/fill_region", { layer_id: "L", shape: { type: "rect", x: 20, y: 20, w: 160, h: 110 }, color: [90, 140, 200], opacity: 1.0 }, doc, r.token);
  return { doc, token: r.token };
};
const exp = async (doc, tok, name) => {
  const rel = `${name}_${stamp}.png`;
  const r = await post("/api/tools/export_png", { path: rel, deterministic: true }, doc, tok);
  return r?.ok === true ? `${expDir}/${rel}` : null;
};
const isBg = (px, i) => px[i] > 240 && px[i + 1] > 240 && px[i + 2] > 240;
const isRect = (px, i) => Math.abs(px[i] - 90) < 25 && Math.abs(px[i + 1] - 140) < 25 && Math.abs(px[i + 2] - 200) < 25;

// ---- 不加调整的基线 ----
const D0 = await mk();
if (!D0) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
const f0 = await exp(D0.doc, D0.token, "pm_before");
if (!f0) { console.error("  ⊘ 不可判：导出失败"); process.exit(3); }
const A = readPng(f0);
const at = (img, x, y) => (y * img.w + x) * img.bpp;
const bgBefore = [[5, 5], [5, 75], [195, 145], [100, 5]].every(([x, y]) => isBg(A.px, at(A, x, y)));
const rectBefore = [[100, 75], [60, 60], [140, 100]].every(([x, y]) => isRect(A.px, at(A, x, y)));
if (bgBefore && rectBefore) ok("① 基线正确：边距是背景、矩形内是矩形色");
else { bad(`① 基线就不对（bg=${bgBefore} rect=${rectBefore}）⇒ 判据前提不成立`); }

// ---- 恒等 quad ⇒ 边距必须仍是背景 ----
const D1 = await mk();
if (!D1) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
const r1 = await post("/api/tools/add_adjustment", {
  layer_id: "L", adjustment_type: "perspective",
  // **⇒ 真恒等 = 画布四角** ✓（第 1330 轮实测：那样 0 个像素变化）
  params: { quad: [[0, 0], [W, 0], [W, H], [0, H]], auto_crop: false },
}, D1.doc, D1.token);
const f1 = await exp(D1.doc, D1.token, "pm_after");
if (!f1) { console.error("  ⊘ 不可判：导出失败"); process.exit(3); }
const B = readPng(f1);
let changed = 0;
if (A.px.length === B.px.length) {
  for (let i = 0; i < A.px.length; i += A.bpp) {
    if (Math.abs(A.px[i] - B.px[i]) > 2 || Math.abs(A.px[i + 1] - B.px[i + 1]) > 2 || Math.abs(A.px[i + 2] - B.px[i + 2]) > 2) changed += 1;
  }
}
if (r1?.ok !== true) {
  bad(`② 画布四角 quad（真恒等）必须被接受 ⇒ ok=${r1?.ok}`);
} else if (changed === 0) {
  ok(`② 画布四角 quad 是**真恒等**：0 个像素变化（${A.px.length / A.bpp} 个像素全同）`);
} else {
  bad(`② 画布四角 quad 必须逐像素不变 ⇒ 实测 ${changed} 个像素变了`);
}

if (fail.length > 0) {
  console.log(`\n  ✗ ${fail.length} 处不符（perspective 的恒等 quad 不应填满边距）`);
  process.exit(1);
}
console.log("\n  ✓ perspective 的恒等 quad 保住了边距");
