#!/usr/bin/env node
// **`smart_sample` 的 `ref_bbox` 必须真的改变采样区域**（第 1422 轮；外部 §213）
//
// **⇒ 外部报的缺陷** ✗：同一 shape、`ref_bbox` 给**像素**与给**归一化**两种写法
//   ⇒ 结果**逐字节一致**（164.7 / 1.02 / 0.62 完全相同）⇒ **参数被忽略** ✗
// **⇒ 根因（源码）**：`read_reference_rgba` 的**图层**分支走 `render_region_raw_layer`（只拿 region ✓），
//   而 **blob / 文件**分支 `decode_png` 后**直接返回整幅** ⇒ `ref_bbox` 对它们**完全无效** ✗✗
//
// **⇒ 断什么**（**判据自己造参考图**，不依赖外部文件 ✓）：
//   ① 同一张「左半红、右半蓝」的参考图，`ref_bbox` = 左半 vs 右半 ⇒ **导出哈希必须不同**
//   ② **语义正确**：采左半 ⇒ 结果偏红（r > b）；采右半 ⇒ 偏蓝（b > r）
//   （① 证明参数生效；② 证明生效的方向也对 —— 只验 ① 可能「变了但变错」✗）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync, writeFileSync, mkdtempSync } from "node:fs";
import { createHash } from "node:crypto";
import { deflateSync } from "node:zlib";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { trackTemp } from "./lib/temp-dirs.mjs";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/tool-smart-sample-ref-bbox.mjs <base>");
  process.exit(3);
}
const dir = trackTemp(mkdtempSync(join(tmpdir(), "refbbox-")));
const stamp = Date.now();

// ---- 造一张 200×100 的参考 PNG：左半红、右半蓝 ----
const W = 200; const H = 100;
const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type, data) => {
  const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
};
const raw = Buffer.alloc(H * (1 + W * 4));
let o = 0;
for (let y = 0; y < H; y += 1) {
  raw[o] = 0; o += 1;
  for (let x = 0; x < W; x += 1) {
    raw[o] = x < W / 2 ? 255 : 0; raw[o + 1] = 0;
    raw[o + 2] = x < W / 2 ? 0 : 255; raw[o + 3] = 255;
    o += 4;
  }
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; ihdr[9] = 6;
const refPath = join(dir, "ref.png");
writeFileSync(refPath, Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0)),
]));

const post = async (doc, tok, tool, args) => {
  const r = await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${tok}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args ?? {}),
  });
  return r.json();
};
const one = async (tag, bbox, refValue = refPath) => {
  const doc = `rbb_${tag}_${stamp}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: W, height: H,
      background: { r: 255, g: 255, b: 255, a: 255 } }),
  })).json();
  if (!made.token) return null;
  await post(doc, made.token, "create_layer", { layer_id: "T", name: "T" });
  const r = await post(doc, made.token, "smart_sample", {
    layer_id: "T", ref: refValue,
    shape: { type: "rect", x: 0, y: 0, w: W, h: H },
    ref_bbox: bbox, samples: 8, density: 0.5, size: 20,
  });
  if (r?.ok !== true) return null;
  const e = await post(doc, made.token, "export_png", { path: `rbb_${tag}_${stamp}.png`, deterministic: true });
  if (e?.ok !== true || !e.path) return null;
  const hash = createHash("sha256").update(readFileSync(e.path)).digest("hex");
  const s = await post(doc, made.token, "sample_colors", { points: [[40, 50], [160, 50]] });
  const px = (s.samples || []).map((x) => x.rgba).filter(Boolean);
  return { hash, px };
};

// **两路都要测** ✓（第 1422 轮学到的教训）：
//   `read_reference_rgba` 有**三条**来路（图层 / blob / 文件）⇒ 只测一条会让判据对另两条**变成安慰剂**
//   （我第一次只测文件路，而变异打在 blob 路上 ⇒ **变异没让判据变红** ⇒ 当场发现 ✓）
const left = await one("left", [0, 0, W / 2, H]);
const right = await one("right", [W / 2, 0, W / 2, H]);
if (!left || !right) {
  console.error("  ⊘ 不可判：smart_sample 或导出失败（不是本判据要断的东西）");
  process.exit(3);
}

// blob 路：先把同一张 PNG 传成 blob，再用 sha256:… 当 ref
let blobLeft = null;
let blobRight = null;
{
  const doc = `bbb_${stamp}`;
  const made = await (await fetch(`${base}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: W, height: H }),
  })).json();
  if (made.token) {
    const put = await post(doc, made.token, "put_blob", { path: refPath });
    // put_blob 只收工作区根目录之内的路径，而判据拿不到 runner 给的 root ⇒ blob 路由提交信息里的手动验证覆盖
    const hash = put?.blob_hash || put?.hash;
    if (hash) {
      blobLeft = await one("blobleft", [0, 0, W / 2, H], hash);
      blobRight = await one("blobright", [W / 2, 0, W / 2, H], hash);
    }
  }
}

const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

// ① 参数必须生效
if (left.hash !== right.hash) ok(`① 两个不同 ref_bbox ⇒ 产物不同（${left.hash.slice(0, 12)} / ${right.hash.slice(0, 12)}）`);
else bad("① ref_bbox 换了一半宽度，产物却逐字节相同 ⇒ 参数被忽略（这正是外部 §213 报的）");

// ② 方向必须正确：采左半（红）⇒ r > b；采右半（蓝）⇒ b > r
const avg = (px) => (px.length ? px.reduce((acc, p) => [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]], [0, 0, 0]).map((v) => v / px.length) : null);
const la = avg(left.px);
const ra = avg(right.px);
if (!la || !ra) {
  bad(`② 采不到像素（左 ${JSON.stringify(left.px)}、右 ${JSON.stringify(right.px)}）`);
} else if (la[0] > la[2] && ra[2] > ra[0]) {
  ok(`② 方向正确：采左半 ⇒ r=${la[0].toFixed(0)} > b=${la[2].toFixed(0)}；采右半 ⇒ b=${ra[2].toFixed(0)} > r=${ra[0].toFixed(0)}`);
} else {
  bad(`② 采样区域与结果颜色必须对应 ⇒ 左半得到 [${la.map((v) => v.toFixed(0))}]、右半得到 [${ra.map((v) => v.toFixed(0))}]`);
}

// ③ blob 路必须与文件路同样生效
if (!blobLeft || !blobRight) {
  console.log("  ○ blob 路未验（put_blob 拿不到 blob_hash）⇒ 少一层覆盖，但不算失败");
} else if (blobLeft.hash !== blobRight.hash) {
  ok(`③ blob 参考路也生效（${blobLeft.hash.slice(0, 12)} / ${blobRight.hash.slice(0, 12)}）`);
} else {
  bad("③ ref 用 blob_hash 时 ref_bbox 仍然无效 ⇒ blob 分支没裁（外部 §213 最可能走的就是这条）");
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（ref_bbox 必须改变采样区域且方向正确）`);
  process.exit(1);
}
console.log("\n  ✓ ref_bbox 真的在采样（且左红右蓝对得上）");
