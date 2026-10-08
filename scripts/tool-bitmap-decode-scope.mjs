#!/usr/bin/env node
// **位图解码不得超出请求区域**判据（第 125 轮 ✓）—— **目标第 1 条（懒分配／按需解码）** 的护栏 ✓
//
// **背景** ✓（第 123 轮实测 ✓）：渲 **1 个 64² tile** ⇒ `bitmap_cache.misses` **每次涨 8** ✗
//（文档有 8 个位图 ⇒ **整幅各解一次** ✗），而该区域**只需其中极小一部分像素** ✓
// ⇒ **∴ 这就是"与区域无关、随文档规模放大"的固定项**（4K 112 ms／8K 400 ms ✗）✓✓
//
// **判据（一条，语义计数 ✓，不看墙钟 ✗）**：
//   **小区域渲染前后，`bitmap_cache.misses` 的增量 ≤ 该区域覆盖的位图数** ✓
//   （区域取 64²（＝1 个 tile ✓）⇒ 覆盖位图数取 **1** ✓ ⇒ **增量必须 ≤ 1** ✓）
//
// **变异** ✗（打在**被判的那一处** ✓）：强制"整幅预解"⇒ 增量变 8 ⇒ **必红** ✓。
//
// **转绿条件** ✓：按请求区域分块解码（GEGL 的 `needRect` ✓）**或**让大位图缓存生效 ✓
//   —— 见 `docs/design/implementation-notes.md` **第 441 轮** ✓。
//
// 用法：node scripts/tool-bitmap-decode-scope.mjs <base-url> [<local.yanshi>]
//
// **⚠️ 为什么必须给一个真实工程** ✗（第 125 轮实测 ✓）：**自建的空文档没有位图** ⇒
// `bitmap_cache.misses` **恒 0** ⇒ 判据**永远 PASS** ✗ ⇒ **∴ 那样它守不住任何东西** ✓。
// ⇒ **∴ 默认加载磁盘上的真实工程**（带位图 ✓），否则**明确报错退出**（**不许静默跳过** ✗）。

import { readFileSync } from "node:fs";

const base = process.argv[2];
const localProject = process.argv[3] || "/tmp/eval/artworks/bench_4k_archive.yanshi";
if (!base) {
  console.error("用法: node scripts/tool-bitmap-decode-scope.mjs <base-url>");
  process.exit(2);
}
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

// **导入真实工程** ✓（带位图 ✓ ⇒ 判据才有意义 ✓）。
let bytes;
try {
  bytes = readFileSync(localProject);
} catch (e) {
  console.error(`✗ 前置不成立：读不到工程 ${localProject} ⇒ ${e.message}`);
  console.error("   用法：node scripts/tool-bitmap-decode-scope.mjs <base-url> <local.yanshi>");
  process.exit(2);
}
const begin = await (await fetch(`${base}/api/documents/import?begin=1`, { method: "POST" })).json();
if (!begin.upload_id) {
  console.error("✗ 前置不成立：入库未开始 ⇒ " + JSON.stringify(begin).slice(0, 160));
  process.exit(2);
}
await fetch(`${base}/api/documents/import?upload=${begin.upload_id}&offset=0`, {
  method: "POST", headers: { "content-type": "application/octet-stream" }, body: bytes,
});
const fin = await (await fetch(`${base}/api/documents/import?upload=${begin.upload_id}&finish=1`, { method: "POST" })).json();
if (!fin.token) {
  console.error("✗ 前置不成立：入库未完成 ⇒ " + JSON.stringify(fin).slice(0, 160));
  process.exit(2);
}
const doc = fin.doc_id;
const token = fin.token;
console.log(`  已导入 ${localProject} ⇒ doc=${doc}`);
const call = async (tool, args) =>
  (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(doc)}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify(args || {}),
  })).json();
const misses = async () => Number(((await (await fetch(`${base}/health`)).json()).bitmap_cache || {}).misses ?? -1);

// **造一个带位图补丁的对象** ✓（这样位图缓存才会被触及 ✓）—— 用 1×1 的极简 raw 补丁 ✓。
const before = await misses();
const r = await call("render_region", { region: [0, 0, 64, 64], include_image: false });
if (r.ok !== true) {
  console.error("✗ 前置不成立：小区域渲染失败 ⇒ " + JSON.stringify(r).slice(0, 160));
  process.exit(2);
}
const after = await misses();
const delta = after - before;
console.log(`  64²（1 个 tile）渲染：bitmap_cache.misses ${before} → ${after}（**增量 ${delta}**）`);

// **核心判据** ✓：区域只覆盖 1 个 tile ⇒ 增量必须 ≤ 1 ✓（**不看墙钟** ✗）
check(delta <= 1,
  "**小区域渲染前后，`bitmap_cache.misses` 增量 ≤ 该区域覆盖的位图数（取 1）**（不许整幅预解 ✗）",
  `增量 ${delta}`);

// **判据②（防退化 ✓，第 127 轮 ✓）**：**"区域渲染必须与整幅渲染在同一区域上逐字节一致"** ✓
// —— **这是项目已有的硬不变量**（"分块与整幅必须一致" ✓）⇒ **∴ 若"按区域剔除位图"剔过头
//（把滤镜真正需要的位图丢掉 ✓）⇒ 两者必有差异 ⇒ 必红** ✓✓（**变异** ✗：全部剔除 ⇒ 必红 ✓）。
const rawAt = async (region) => {
  const value = await call("render_region", { region, raw: true });
  let url = value.raw_url || value.thumb_url;
  if (!url) throw new Error("拿不到 raw_url ⇒ " + JSON.stringify(value).slice(0, 140));
  if (String(url).startsWith("yanshi://blob/")) {
    url = `${base}/api/blob/${String(url).split("/").pop()}?doc=${encodeURIComponent(doc)}`;
  } else if (String(url).startsWith("/")) {
    url = base + url;
  }
  const res = await fetch(String(url));
  if (!res.ok) throw new Error(`取 raw 失败 HTTP ${res.status}`);
  return { bytes: Buffer.from(await res.arrayBuffer()), w: value.width, h: value.height };
};

// 取"整幅"尺寸以做对照 ✓（先问一次整幅；区域取左上 64² ✓ 以保证在画幅内 ✓）
// **⚠️ 画幅要从文档列表取** ✗ —— 响应里的 `width`／`height` 是**请求区域**的尺寸 ✓（我第一版弄错了 ✗）。
const listed = await (await fetch(`${base}/api/documents`)).json();
const info = (listed.documents || []).find((d) => d.doc_id === doc) || {};
const W = Number(info.width), H = Number(info.height);
if (!(W > 128 && H > 128)) {
  console.error(`✗ 前置不成立：画幅太小 ${W}×${H}（需真实工程 ✓）`);
  process.exit(2);
}
const tiny = await rawAt([0, 0, 64, 64]);
const whole = await rawAt([0, 0, W, H]);
const rowBytesTiny = 64 * 4;
let mismatches = 0;
for (let y = 0; y < 64; y++) {
  const a = tiny.bytes.subarray(y * rowBytesTiny, (y + 1) * rowBytesTiny);
  const b = whole.bytes.subarray((y * W) * 4, (y * W) * 4 + rowBytesTiny);
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) mismatches++;
}
console.log(`  逐字节比对：区域 64²（${tiny.bytes.length} B）vs 整幅 ${W}×${H} 的同位块 ⇒ **不一致字节 ${mismatches}**`);
check(mismatches === 0,
  "**区域渲染与整幅渲染在同一区域上必须逐字节一致**（防「剔除过头」把位图丢掉 ✗）",
  `不一致 ${mismatches} 字节`);

console.log("");
if (failures.length) {
  console.error(`结论：位图解码**超出请求区域** ✗（${failures.length} 条）—— 尚未实现按需解码 ✓（预期红 ✓）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 位图解码被限制在请求区域内");
process.exit(0);
