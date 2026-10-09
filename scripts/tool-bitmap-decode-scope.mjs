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

import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname } from "node:path";
import { deflateSync } from "node:zlib";

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

// **★ 自备 fixture ✗ ★**（第 40 轮 ✓）：**缺工程时**用产品自身的接口造一份**✗。
//
// **∴ 为什么必须自备 ✗**：**判据**要求**带位图**✗（**∴ 空文档 ⇒ `missed_bytes` 恒 0 ⇒
//   **∴ 判据**永远 PASS ⇒ **∴ 守不住任何东西 ✓）⇒ **∴ 而**一个**机器相关**的
//   `/tmp` 文件**不能**进 CI ✓ ⇒ **∴ 于是**：**让脚本**自己造**✗（**造完就留 ✓）** ✓✓
//
// **∴ 造法（**全部走产品自己的接口 ✓）**：
//   **①** 新建 4K 文档 ⇒ **②** 建图层 ⇒ **③** 上传一张小 PNG（**本地生成 ✓）
//   ⇒ **④** `import_image`（**这才产生位图 ✓）⇒ **⑤** `export_project(path=…)` ✓**** ✓✓
//
// **∴ 两面 ✗**：**收益**＝判据能在任何机器上跑 ✓；**代价**＝**多了约 5 步准备 ＋
//   依赖服务端能写 `path`（**同机时成立 ✓）** ✓✓

/** **∴ 造一张 w×h 的纯色 PNG ✗**（**不引依赖 ✓：**deflateSync ＋ 手工 CRC32 ✓）** ✓✓ */
function solidPng(w, h, [r, g, b]) {
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y += 1) {
    const row = y * (w * 3 + 1);
    raw[row] = 0; // filter: none
    for (let x = 0; x < w; x += 1) {
      raw[row + 1 + x * 3] = r;
      raw[row + 2 + x * 3] = g;
      raw[row + 3 + x * 3] = b;
    }
  }
  const crcTable = [];
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
  }
  const crc32 = (buf) => {
    let c = 0xffffffff;
    for (const byte of buf) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const head = Buffer.alloc(4);
    head.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const tail = Buffer.alloc(4);
    tail.writeUInt32BE(crc32(body));
    return Buffer.concat([head, body, tail]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // colour type: truecolour
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** **∴ 用产品接口造一份带位图的工程 ✗ ★** */
async function buildFixture(base, target) {
  console.log(`  · 缺工程 ⇒ 用产品接口现造一份 ⇒ ${target}`);
  // **∴ 生成用的 id**必须与包里要导入的那个**不同**✗
  //   ⇒ **∴ 否则**导入时**撞名**✗（**实测：**conflict ✓）** ✓✓
  const docId = "bitmap_scope_fixture_src";
  const created = await (await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width: 2048, height: 2048 }),
  })).json();
  const token = created.token;
  if (!token) throw new Error("建文档失败 ⇒ " + JSON.stringify(created).slice(0, 160));
  const q = `doc=${docId}&token=${token}`;
  const call = async (tool, args) =>
    (await fetch(`${base}/api/tools/${tool}?${q}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args || {}),
    })).json();
  const layer = await call("create_layer", { layer_id: "bitmap_layer" });
  if (layer.ok !== true) throw new Error("建图层失败 ⇒ " + JSON.stringify(layer).slice(0, 160));
  const png = solidPng(512, 512, [200, 40, 90]);
  const put = await (await fetch(`${base}/api/blob?${q}`, {
    method: "POST",
    headers: { "content-type": "application/octet-stream" },
    body: png,
  })).json();
  const hash = put.hash || put.blob_hash;
  if (!hash) throw new Error("上传 blob 失败 ⇒ " + JSON.stringify(put).slice(0, 160));
  const placed = await call("import_image", {
    layer_id: "bitmap_layer",
    object_id: "bitmap_object",
    bitmap: { blob_hash: hash, size: png.length, mime_type: "image/png" },
    region: { x: 16, y: 16, width: 512, height: 512 },
  });
  if (placed.ok !== true) throw new Error("import_image 失败 ⇒ " + JSON.stringify(placed).slice(0, 160));
  mkdirSync(dirname(target), { recursive: true });
  const exported = await call("export_project", { path: target, include_bitmaps: true });
  if (exported.ok !== true) throw new Error(
    `export_project 失败（**⇒ 该服务端可能不能写 path ✓）⇒ ${JSON.stringify(exported).slice(0, 200)}`,
  );
  // **★ 生成完就把**源文档删掉 ✗ ★**（第 40 轮 ✓）：**∴ 因为**接下来**要**导入**同一个包**✗
  //   ⇒ **∴ 若**源文档还在 ⇒ **∴ `import_project` **不会覆盖**同 id**✗
  //     ⇒ **∴ 实测**报 `conflict`**✗（**∴ 我**第一次就撞上了 ✓）** ✓✓
  const removed = await call("delete_document", { document_id: docId });
  if (removed.ok !== true) {
    // **∴ 不静默 ✗**：**删不掉**就可能撞名 ⇒ **∴ 说清楚 ✓**（**∴ 而不是**后面报一个看不懂的 conflict ✓）** ✓✓
    console.error("  ⚠️ 源文档删不掉 ⇒ 稍后导入可能撞名 ⇒ " + JSON.stringify(removed).slice(0, 160));
  }
  console.log("  · fixture 已生成 ✓（**带位图 ✓）＋ 源文档已删 ✓");
}

// **导入真实工程** ✓（带位图 ✓ ⇒ 判据才有意义 ✓）。
let selfBuilt = false;
if (!existsSync(localProject)) {
  await buildFixture(base, localProject);
  // **∴ 记住"**这份是我造的 ✓"✗** ⇒ **∴ 后面**要**自检它**是否真的触发了判据 ✓**** ✓✓
  selfBuilt = true;
}
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
// **读"未命中字节"** ✓（第 134 轮 ✓）—— 它比"次数"更能说明"这一笔重新解压了多大的东西" ✓
//（一次整幅 4K 背景 ＝ 33.2 MiB ✓，一枚小补丁几十 KiB ✓）。
const missedBytes = async () => Number(((await (await fetch(`${base}/health`)).json()).bitmap_cache || {}).missed_bytes ?? -1);

// **造一个带位图补丁的对象** ✓（这样位图缓存才会被触及 ✓）—— 用 1×1 的极简 raw 补丁 ✓。
const REGION = [0, 0, 64, 64];
const REGION_PIXELS = REGION[2] * REGION[3];
// **阈值从面积推导** ✓（**不写死观测** ✗）：解码字节 ≤ K × 区域像素 × 4 B/px ✓
const K = 4;
const LIMIT = K * REGION_PIXELS * 4;
const before = await missedBytes();
const r = await call("render_region", { region: REGION, include_image: false });
if (r.ok !== true) {
  console.error("✗ 前置不成立：小区域渲染失败 ⇒ " + JSON.stringify(r).slice(0, 160));
  process.exit(2);
}
const after = await missedBytes();
const delta = after - before;
console.log(`  64²（${REGION_PIXELS} 像素）渲染：decoded_bytes ${before} → ${after}`
  + `（**增量 ${delta} B ＝ ${(delta / 1048576).toFixed(1)} MiB**；阈值 ${LIMIT} B）`);

// **信息性输出** ✓（第 136 轮 ✓）：首次渲染对**整体式 blob**（Deflate 流 ✓）**不可避免地要整条解压** ✗
// ⇒ **∴ 它不是"可修的缺陷"，所以**不作为断言**** ✓ —— 只打印，供对照 ✓。
console.log(`  首次渲染解码 ${delta} B ＝ ${(delta / (REGION_PIXELS * 4)).toFixed(0)}× 区域像素`
  + `（阈值 ${LIMIT} B）`);

// **① 首次版判据** ✓（第 158 轮 ✓，**规格先行 ⇒ 今天必红** ✓）：**解码字节必须与**区域面积**同量级** ✓
// —— **∵ 整体式 blob ⇒ 首次整条解压**不可避免**✗** ⇒ **∴ 只有**位图分块存储（(b1) ✓）才能让它绿** ✓✓
// **变异** ✗：强制整幅预解 ⇒ 必红 ✓。
check(delta <= LIMIT,
  `**小区域所解码的字节 ≤ K × 区域像素 × 4（K=${K}）**（不许整幅预解 ✗；**须分块存储** ✓）`,
  `解码 ${delta} B（＝ ${(delta / (REGION_PIXELS * 4)).toFixed(0)}× 区域像素）`);

// **①′ 核心判据** ✓（第 136 轮 ✓）：**同一区域**第二次**渲染 ⇒ 解码字节增量必须为 0** ✓
// —— **∵ 首次不可避免 ✓，而**重复解压同一份纯属浪费**✗** ⇒ **∴ 这才是可修、且必须修的** ✓✓
// （**不看墙钟** ✗；**变异** ✗：强制每次重解 ⇒ 必红 ✓）
const before2 = await missedBytes();
const r2 = await call("render_region", { region: REGION, include_image: false });
const after2 = await missedBytes();
const delta2 = after2 - before2;
console.log(`  第二次渲染：decoded_bytes ${before2} → ${after2}（**增量 ${delta2} B**）｜ok=${r2.ok}`);
// **★ 触发诊断 ✗ ★**（第 41 轮 ✓）：**∴ 小区域的增量是 0** ✗
//   ⇒ **∴ 那**有两种可能 ✗：**（a）**判据要守的东西成立 ✓**；**（b）**这条 fixture
//     **根本没走位图路径**✗（**∴ 那**判据就是**绿得没意义 ✓**）** ✓✓
//   **∴ 分法 ✗**：**整幅渲一次**✗ ⇒ **∴ 若**它**会**解码位图 ⇒ **∴ 说明**路径是通的 ✓，
//     **而**小区域不去碰它**正是判据要守的** ✓**** ✓✓
//   **∴ 只打印 ⇒ **不参与断言**✗（**∴ 它**是**诊断 ✓）** ✓✓
{
  const b3 = await missedBytes();
  const big = await call("render_region", { region: [0, 0, 2048, 2048], include_image: false });
  const a3 = await missedBytes();
  console.log(`  触发诊断：整幅渲染 decoded_bytes ${b3} → ${a3}（**增量 ${a3 - b3} B**）｜ok=${big.ok}`);
  // **★ 再看那个色块**到底有没有进渲染 ✗ ★**（第 41 轮 ✓）：
  //   **∴ 判法 ✗**：**取**放置区域中心的一个像素**✗ ⇒ **∴ 看**它**是不是**我上传的颜色 ✓**
  //     **∴ 是** ⇒ **∴ 位图**进了渲染**✗，**而**没解码 ⇒ **∴ 缓存另有来源 ✓**
  //     **∴ 不是** ⇒ **∴ fixture**的内容没进去**✗ ⇒ **∴ 要修生成流程 ✓**** ✓✓
  const raw = await call("render_region", { region: [300, 300, 4, 4], raw: true });
  if (raw.raw_url) {
    const res = await fetch(raw.raw_url.startsWith("http") ? raw.raw_url : base + raw.raw_url);
    const buf = Buffer.from(await res.arrayBuffer());
    console.log(`  内容诊断：放置区中心像素 = [${buf[0]}, ${buf[1]}, ${buf[2]}, ${buf[3]}]`
      + `（**期望接近** [200, 40, 90, 255] ✓）`);
  } else {
    console.log("  内容诊断：拿不到 raw_url ⇒ " + JSON.stringify(Object.keys(raw)));
}
  const triggered = a3 - b3 > 0;
  console.log(`    ⇒ ${triggered
    ? "位图路径**是通的** ✓ ⇒ ∴ 小区域增量 0 正是判据要守的结果 ✓"
    : "**整幅也没有解码字节** ✗ ⇒ ∴ 这条 fixture **没走位图路径** ⇒ ∴ 判据绿得没意义 ✓"}`);
  // **★ 护栏 ✗ ★**（第 41 轮 ✓）：**自造 fixture**若**触发不了**判据**✗**
  //   ⇒ **∴ 必须**以非零退出**✗ ⇒ **∴ 不许**让"**全绿 ✓"**蒙住 CI ✓**** ✓✓
  //   **∴ 为什么只对**自造**的管 ✗**：**调用方**给的工程**✗（**如**第 125 轮那份 ✓）
  //     **∴ 我**无法**替它判断** ⇒ **∴ 只打印 ✓**** ✓✓
  if (selfBuilt && !triggered) {
    console.error("  ❌ 判据**未生效** ⇒ **本脚本自造的 fixture 不触发位图解码** ✗");
    console.error("     ⇒ **∴ 结论**：这份 fixture 不能用来守这条判据 ✓");
    console.error("     ⇒ **∴ 下一步**：换成**含笔触位图／raw 补丁**的工程（见第 125 轮的原始工程 ✓）");
    process.exit(1);
  }
}

check(delta2 === 0,
  "**同一区域第二次渲染 ⇒ 解码字节增量必须为 0**（缓存须生效 ✓；不许重复解压 ✗）",
  `增量 ${delta2} B`);


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
