#!/usr/bin/env node
// **照片修图四个新调整的判据**（第 1316 轮；合并 `redeye-jpeg` + `photoretouch` 补丁后补的）
//
// **⇒ 为什么要有这条** ✗：补丁加了四个调整（`red_eye` / `denoise` / `lens` / `perspective`），
//   而**一条判据都没有** ⇒ 所以「它们能 add」只证明**参数被接受**，不证明**有效果** ✓
//
// **⇒ 断什么**（每个调整一个**可量的方向性**断言，不是「像素数等于多少」）：
//   ① `red_eye`：红色斑块 ⇒ 加调整后**最大红通道下降**（红眼被压）
//   ② `denoise`：噪声区 ⇒ 加调整后**局部标准差下降**（噪声被抹）
//   ③ `lens`（`k1 != 0`）：栅格 ⇒ 加调整后**至少有 N 个像素变化**（几何动了）
//   ④ `perspective`（`quad`）：栅格 ⇒ 加调整后**至少有 N 个像素变化**（透视动了）
//
// **⇒ 为什么断方向而不是数值**：具体数值取决于算法常数 ⇒ 断方向跨机器稳定 ✓
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { join } from "node:path";
import zlib from "node:zlib";
import { readFileSync } from "node:fs";

const [base, , tokenArg] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-photo-adjustments.mjs <base> <doc> <token>");
  process.exit(3);
}
const stamp = Date.now();
const fail = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fail.push(m); console.log(`  ✗ ${m}`); };

const post = async (path, body, doc, token) => {
  const url = `${base}${path}${doc ? `?doc=${encodeURIComponent(doc)}&token=${encodeURIComponent(token)}` : ""}`;
  const r = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body ?? {}),
    signal: AbortSignal.timeout(120_000),
  });
  return await r.json();
};

// ---- PNG 读取（8-bit RGB/RGBA，非隔行）----
const readPng = (file) => {
  const buf = readFileSync(file);
  let pos = 8; let w = 0; let h = 0; let ct = 0; const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos);
    const t = buf.toString("ascii", pos + 4, pos + 8);
    const d = buf.subarray(pos + 8, pos + 8 + len);
    if (t === "IHDR") { w = d.readUInt32BE(0); h = d.readUInt32BE(4); ct = d[9]; }
    if (t === "IDAT") idat.push(d);
    pos += 12 + len;
  }
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const bpp = ct === 6 ? 4 : 3; const stride = w * bpp; const out = Buffer.alloc(h * stride);
  for (let y = 0; y < h; y += 1) {
    const f = raw[y * (stride + 1)];
    const src = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    for (let x = 0; x < stride; x += 1) {
      const a = x >= bpp ? out[y * stride + x - bpp] : 0;
      const b = y > 0 ? out[(y - 1) * stride + x] : 0;
      const c = x >= bpp && y > 0 ? out[(y - 1) * stride + x - bpp] : 0;
      let v = src[x];
      if (f === 1) v += a;
      else if (f === 2) v += b;
      else if (f === 3) v += (a + b) >> 1;
      else if (f === 4) {
        const p = a + b - c; const pa = Math.abs(p - a); const pb = Math.abs(p - b); const pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      out[y * stride + x] = v & 255;
    }
  }
  return { w, h, bpp, px: out };
};

const mkDoc = async (tag, w, h) => {
  const doc = `pa_${tag}_${stamp}`;
  const r = await post("/api/documents", { doc_id: doc, width: w, height: h });
  if (!r?.token) return null;
  await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, r.token);
  return { doc, token: r.token };
};

const exportPng = async (doc, token, name) => {
  // **⇒ 不自己造临时目录** ✓（元判据要求造目录必须 import trackTemp ⇒ 所以用运行器给的）
  // **⇒ 候选路径链** ✓（第 1317 轮）：本机运行器给 `YANSHI_EXPORT_DIR`；
  //   **而 CI 的 `run-criteria.sh` 只导出 `YANSHI_WORKSPACE`**（服务端写 `$ROOT/work/exports`）✗
  //   ⇒ 若只认 `YANSHI_EXPORT_DIR` ⇒ 本判据**在 CI 上永远 `exit 3`（跳过）** ⇒ 等于没加 ✗
  //   ⇒ 所以与 `tool-cjk-text` 同一套候选链 ✓
  // **⇒ 顺序取，**不要求已存在**** ✓（第 1317 轮实测）：
  //   我第一版用 `existsSync` 筛 ⇒ 而「本机运行器刚建好 `exports/` 但服务端还没写」时
  //   `${ws}/exports` 不存在 ⇒ 于是退到 `ws` ⇒ **与服务端的写路径不一致** ⇒ 读不到文件 ✗
  //   ⇒ 所以：`YANSHI_EXPORT_DIR` **有值就用**（它就是服务端的导出目录）；
  //     否则用 `${YANSHI_WORKSPACE}/exports`（CI 的 `run-criteria.sh` 就是那个布局）✓
  const ws = process.env.YANSHI_WORKSPACE;
  const dir = process.env.YANSHI_EXPORT_DIR || (ws ? `${ws}/exports` : null);
  if (!dir) {
    console.error("  ⊘ 不可判：找不到导出目录（试过 YANSHI_EXPORT_DIR / YANSHI_WORKSPACE/exports / YANSHI_WORKSPACE）");
    process.exit(3);
  }
  const rel = `${name}_${stamp}.png`;
  const r = await post("/api/tools/export_png", { path: rel, deterministic: true }, doc, token);
  if (r?.ok !== true) return null;
  return join(dir, rel);
};

// ---- ① red_eye：红点 ⇒ 最大红通道下降 ----
{
  const D = await mkDoc("re", 60, 60);
  if (!D) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
  await post("/api/tools/fill_region", {
    layer_id: "L", shape: { type: "rect", x: 20, y: 20, w: 20, h: 20 },
    color: [220, 20, 20], opacity: 1.0,
  }, D.doc, D.token);
  const before = await exportPng(D.doc, D.token, "re_before");
  await post("/api/tools/add_adjustment", {
    layer_id: "L", adjustment_type: "red_eye", params: { strength: 1.0, threshold: 2.0 },
  }, D.doc, D.token);
  const after = await exportPng(D.doc, D.token, "re_after");
  if (!before || !after) { console.error("  ⊘ 不可判：导出失败"); process.exit(3); }
  // **★ 只量**红点中心**（第 1316 轮实测）**：全图最大红恒 255（白底是 `[255,255,255]`）
  //   ⇒ 那样永远测不出变化 ✗ ⇒ 所以采 `(30, 30)`（红块 20..40 的中心）✓
  const centreRed = (f) => {
    const { bpp, w, px } = readPng(f);
    return px[(30 * w + 30) * bpp];
  };
  const b = centreRed(before); const a = centreRed(after);
  if (a < b) ok(`① red_eye 压低红点中心：${b} → ${a}`);
  else bad(`① red_eye 必须压低红点中心（30,30）⇒ 实测 ${b} → ${a}（没效果）`);
}

// ---- ② denoise：噪声区 ⇒ 局部标准差下降 ----
{
  const D = await mkDoc("dn", 60, 60);
  if (!D) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
  // 交替色块 = 高频噪声（确定性，不用随机数）
  for (let i = 0; i < 12; i += 1) {
    const c = i % 2 === 0 ? [40, 40, 40] : [210, 210, 210];
    await post("/api/tools/fill_region", {
      layer_id: "L", shape: { type: "rect", x: 10 + i * 3, y: 10, w: 3, h: 40 }, color: c, opacity: 1.0,
    }, D.doc, D.token);
  }
  const std = (f) => {
    const { bpp, w, px } = readPng(f);
    let s = 0; let s2 = 0; let n = 0;
    for (let y = 12; y < 48; y += 1) {
      for (let x = 12; x < 46; x += 1) { const v = px[(y * w + x) * bpp]; s += v; s2 += v * v; n += 1; }
    }
    const mean = s / n; return Math.sqrt(Math.max(0, s2 / n - mean * mean));
  };
  const before = await exportPng(D.doc, D.token, "dn_before");
  await post("/api/tools/add_adjustment", {
    layer_id: "L", adjustment_type: "denoise", params: { luminance: 0.9, detail: 0.9, color: 0.5 },
  }, D.doc, D.token);
  const after = await exportPng(D.doc, D.token, "dn_after");
  if (!before || !after) { console.error("  ⊘ 不可判：导出失败"); process.exit(3); }
  const b = std(before); const a = std(after);
  if (a < b - 0.5) ok(`② denoise 降低局部标准差：${b.toFixed(1)} → ${a.toFixed(1)}`);
  else bad(`② denoise 必须降低局部标准差 ⇒ 实测 ${b.toFixed(1)} → ${a.toFixed(1)}`);
}

// ---- ③ lens / ④ perspective：几何必须动 ----
const changedPixels = (f1, f2) => {
  const A = readPng(f1); const B = readPng(f2);
  if (A.w !== B.w || A.h !== B.h) return 0;
  let n = 0;
  for (let i = 0; i < A.px.length; i += A.bpp) {
    if (Math.abs(A.px[i] - B.px[i]) > 6 || Math.abs(A.px[i + 1] - B.px[i + 1]) > 6) n += 1;
  }
  return n;
};
const geoCase = async (tag, kind, params, label) => {
  const D = await mkDoc(tag, 60, 60);
  if (!D) return;
  await post("/api/tools/fill_region", {
    layer_id: "L", shape: { type: "rect", x: 5, y: 5, w: 50, h: 50 }, color: [90, 140, 200], opacity: 1.0,
  }, D.doc, D.token);
  await post("/api/tools/fill_region", {
    layer_id: "L", shape: { type: "rect", x: 25, y: 25, w: 10, h: 10 }, color: [240, 230, 60], opacity: 1.0,
  }, D.doc, D.token);
  const before = await exportPng(D.doc, D.token, `${tag}_before`);
  const r = await post("/api/tools/add_adjustment", {
    layer_id: "L", adjustment_type: kind, params,
  }, D.doc, D.token);
  const after = await exportPng(D.doc, D.token, `${tag}_after`);
  if (!before || !after) { console.error("  ⊘ 不可判：导出失败"); process.exit(3); }
  const n = changedPixels(before, after);
  if (r?.ok === true && n > 200) ok(`${label}：${n} 个像素变化（几何确实动了）`);
  else bad(`${label}：ok=${r?.ok}、变化 ${n} 个像素（应 > 200）`);
};
await geoCase("ln", "lens", { k1: 0.35, auto_crop: false }, "③ lens(k1=0.35)");
await geoCase(
  "pp", "perspective",
  { quad: [[8, 8], [52, 4], [56, 56], [4, 52]], auto_crop: false },
  "④ perspective(quad)",
);

// ---- ⑤ 幂等：`lens` 全缺省参数必须**什么都不改** ----
//   **⇒ 为什么这是一条**不变量**而不只是「另一个 case」 ✗：
//     「几何调整在参数全 0 时不动像素」是**语义承诺** ⇒ 若它恒动（哪怕 1 像素）
//     ⇒ 说明重采样底座**无条件跑**（如 `auto_crop` 或钳制总在生效）⇒ 那会
//     让**每一层**带 lens 的文档都在无声地被重采样一次 ✗
//   **⇒ 实测（第 1319 轮）：0 个像素变化、最大通道差 0** ✓
{
  const D = await mkDoc("idem", 60, 60);
  if (!D) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
  await post("/api/tools/fill_region", {
    layer_id: "L", shape: { type: "rect", x: 5, y: 5, w: 50, h: 50 }, color: [90, 140, 200], opacity: 1.0,
  }, D.doc, D.token);
  const before = await exportPng(D.doc, D.token, "idem_before");
  const r = await post("/api/tools/add_adjustment", {
    layer_id: "L", adjustment_type: "lens", params: {},
  }, D.doc, D.token);
  const after = await exportPng(D.doc, D.token, "idem_after");
  if (!before || !after) { console.error("  ⊘ 不可判：导出失败"); process.exit(3); }
  const n = changedPixels(before, after);
  if (r?.ok === true && n === 0) ok("⑤ 缺省 lens 幂等：0 个像素变化（几何底座不会无条件重采样）");
  else bad(`⑤ 缺省 lens 必须幂等 ⇒ ok=${r?.ok}、变化 ${n} 个像素（应 0）`);
}

if (fail.length > 0) {
  console.log(`\n  ✗ ${fail.length} 处不符（四个照片调整必须各有可量效果）`);
  process.exit(1);
}
console.log("\n  ✓ 四个照片调整都有可量效果（红眼压低 / 噪声下降 / 两种几何都在动）");
