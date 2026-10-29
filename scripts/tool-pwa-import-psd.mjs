#!/usr/bin/env node
// **判据：PWA 本地的 `import_psd` 与服务端同答案（PSD 合成图逐字节一致）** ✓（台账 §673）。
//
// **∴ 它测什么 ✗**（**内核 ①：断言必须能证明自己能红 ✓**）：
//   同一份 PSD 字节分别上传到真服务端与本地 `api-local` ✓ ⇒ 调 `import_psd` ⇒ 比**日志里的
//   `import_image` 原子的净荷** ✓（**服务端回执顶层没有 blob_hash/object_id ⇒ 权威形状就是原子净荷 ✓**）。
//   **∴ 最强的那条是恒等式 ✗**：合成图的 `bitmap.blob_hash` 是**内容寻址的 sha256** ⇒
//   **两边哈希逐字符相等 ⇔ 解码出的 RGBA 逐字节相等** ✓（**不需要把像素读回来比 ✓**）；
//   **∴ 但光比两边不够**（两边同错时相等照样绿 ⇒ 安慰剂 ✗）⇒
//   **每条成功路径都另断一个判据自己按输入算出的期望** ✓：
//   `source.width/height` 与夹具的真实宽高 ✓、`object_id` 回显 ✓、
//   `blob_decodable === true`（**mime 是 `image/x-yanshi-raw` ⇒ 服务端也恒 true ✓**）。
//   **∴ 三条互补 ✗**：哈希相等抓「两边不一致」✓、独立期望抓「两边同错」✓、
//   错误逐字比抓「文案／状态码／先后顺序分叉」✓（**§G4 那条就是真分叉的探测器 ✓**）。
//
// **∴ 夹具是两个来源 ✗**（**内核 ①：输入必须制造出要判的情况 ✓**）：
//   ① 仓库自带 `crates/yanshi-render/tests/data/psd/agpsd-two-layers.psd`
//      （**实测：版本 1／8 位／RGB／3 通道／压缩 0=原始／64×64 ✓** —— **服务端解码器的权威输入 ✓**）；
//   ② 判据自带的**构造器**（**逐字照 `psd.rs:209 encode_psd` ✓**）⇒
//      覆盖 ① 没有走到的支：`compression=1(RLE)` ✓、`channels=4`（**alpha 平面 ✓**）✓、
//      `channels=1`（**g、b 复制 r、alpha=255 的缺省支 ✓**）✓、以及全部拒绝支 ✓。
//
// **∴ 覆盖的面 ✗→✓**：
//   §A 上传口径（**服务端与本地对同一份字节给同一个 sha256 ✓** —— 不一致则整套判据前提不成立 ⇒ `exit(3)` ✓）；
//   §B 真夹具成功路径（哈希相等／回显／`seq` 推进／终态 `describe_canvas` 对象计数一致 ✓）；
//   §C RLE×3 通道、§D RLE×4 通道（alpha 逐像素 0/40/80/120/160/200 ✓）、§E 原始×1 通道（灰复制 ✓）；
//   §F 拒绝支逐字（位深 16 ✓／色彩模式 4 ✓／版本 2 ✓／压缩 2 ✓／截断 ✓／不是 PSD ✓／太短 ✓／
//      RLE 行解码偏短 ⇒「解压后」守卫 ✓）；
//   §G 入口链（缺 layer_id ✓／缺 blob_hash ✓／类型错 ✓／未知键 ✓／非法哈希 ✓／
//      **规范化的大写哈希两边都拒 ✓ —— 旧 `p2BlobHashOk` 收大写，本分支不用它 ✓**／
//      不存在的 blob ⇒ 404 文案是**规范化后的哈希** ✓／图层不存在 ✓）。
//
// **∴ 前提 ✗**：需要**活着的服务端**（`run-criteria-local.sh` 起并把地址放进 argv[2] ✓）
//   ＋ 仓库夹具在位 ＋ 本地内核（`blob_put` 取内容哈希 ✓）。拿不到 ⇒ `exit(3)`（≠ 产品失败 ✓）。
//   服务端拒绝前序 ⇒ 同样 `exit(3)` ✓；本地抛异常 ⇒ 记**产品失败** ✓。
//
// **∴ 变异（逐条对应断言 ✓，实测记录见台账「第 六百七十三 节」）**：
//   ① PackBits 原样段 `count = control`（少 +1）⇒ §C/§D 哈希不等红 ✓；
//   ② 通道 1 时 `g = 0`（不复制 r）⇒ §E 红 ✓；
//   ③ 通道 <4 时 `alpha = 0`⇒ §C/§E 红 ✓；
//   ④ 签名检查改成恒真 ⇒ §F6 红 ✓；
//   ⑤ 压缩方式表加成 `2` 也支持 ⇒ §F4 红 ✓；
//   ⑥ 长度跳段时连 4 字节长度一起跳掉（`at += len` 少 `+4`）⇒ §B 红 ✓；
//   ⑦ 「解压后」守卫（RLE 行解码偏短）改成恒不触发 ⇒ §F8 本地 200 红 ✓
//     （**∴ 第 673 轮实测教训 ✗**：原打算用 §F5 截断案来红这条，实测 §F5 在更早的
//      「原始压缩越界」那关就拦住了 ⇒ 「解压后」守卫**没有输入能触发** ⇒ 补 §F8 才齐 ✓）；
//   ⑧ 合成 blob 只进内核、不写 IndexedDB（漏 `putBlob` ⇒ 刷新后重放引用悬空）⇒ §B–§E 本地读回红 ✓；
//   ⑨ 404 文案回显用户原样写法（不规范化）⇒ §G6 红 ✓；
//   ⑩ `source.width` 写成 0 ⇒ §B 独立期望红 ✓；
//   ⑪ 入口校验改成只查 blob_hash（跳过声明顺序）⇒ §G1 红 ✓。
process.on("uncaughtException", (error) => {
  console.error(String(error && error.stack || error).slice(0, 1200));
  process.exit(3);
});
import { pathToFileURL } from "node:url";
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const BASE = process.argv[2];
if (!BASE || !/^https?:\/\//.test(BASE)) {
  console.log("⊘ 不可判：argv[2] 缺服务端地址 ⇒ 请用 bash scripts/run-criteria-local.sh scripts/tool-pwa-import-psd.mjs 跑"
    + "（判据前提不成立，不是产品问题）");
  process.exit(3);
}

const bad = [];
let okCount = 0;
const check = (cond, msg) => { if (cond) okCount += 1; else bad.push(msg); };
const show = (value) => String(JSON.stringify(value)).slice(0, 300);
const UNIQ = "psd" + Date.now().toString(36);
const SERVER_DOC = `psdsrv-${UNIQ}`;
const LOCAL_DOC = `psdloc-${UNIQ}`;
const REPO = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const FIXTURE = path.join(REPO, "crates/yanshi-render/tests/data/psd/agpsd-two-layers.psd");

// **∴ 必须用真内核桩 ✗**（`blob_put` 取内容哈希是这条判据恒等式的地基 ✓ —— 普通桩的
//   `__loaders.wasm()` 在 node 里导不到 `/wasm/…` ⇒ 本地 `/api/blob` 直接 501 ⇒ 前提不成立 ✓）。
const kernelLib = await import(pathToFileURL(process.cwd() + "/scripts/lib/pwa-node-kernel.mjs").href);
const staleness = kernelLib.kernelStaleness();
if (!staleness.fresh) {
  console.log(`⊘ 不可判：${staleness.reason}`);
  for (const line of staleness.stale) console.log("   · " + line);
  console.log("   ⇒ 判据测到的会是上一版内核 ⇒ 前提不成立，不是产品问题");
  process.exit(3);
}
const { local: localApi } = await kernelLib.bootLocalWithRealKernel({ origin: BASE });
const nativeFetch = globalThis.fetch;

// ══ 服务端文档 ══
const createdBody = await nativeFetch(`${BASE}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: SERVER_DOC, width: 128, height: 128 }),
}).then((r) => r.json());
if (createdBody.ok !== true || !createdBody.token) {
  console.log(`⊘ 不可判：服务端建文档失败（${show(createdBody)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}
const TOKEN = createdBody.token;
const localCreated = await localApi(new Request(`https://x/api/documents?doc=${LOCAL_DOC}`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: LOCAL_DOC, width: 128, height: 128 }),
})).then((r) => r.json().catch(() => null));
if (!localCreated || localCreated.ok !== true) {
  console.log(`⊘ 不可判：本地建文档失败（${show(localCreated)}）⇒ 前提不成立，不是产品问题`);
  process.exit(3);
}
if (!fs.existsSync(FIXTURE)) {
  console.log(`⊘ 不可判：仓库夹具缺失 ${FIXTURE}（先 cargo test -p yanshi-render write_fixture_for_criteria）⇒ 前提不成立`);
  process.exit(3);
}

const callServer = async (tool, args) => {
  const res = await nativeFetch(`${BASE}/api/tools/${tool}?doc=${SERVER_DOC}&token=${TOKEN}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: SERVER_DOC, actor: "human:judge", session: "session:judge", ...args }),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
};
/** **本地抛异常记成产品失败 ✗ 不是判据坏了 ✓**（与 `tool-pwa-describe-canvas` 同一纪律 ✓）。 */
const localCall = async (tool, args) => {
  try {
    const res = await localApi(new Request(`https://x/api/tools/${tool}?doc=${LOCAL_DOC}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
    }));
    return { status: res.status, body: await res.json().catch(() => null) };
  } catch (e) {
    return { status: 0, body: { ok: false, error_code: "local_threw",
      context: { detail: String((e && e.message) || e).slice(0, 300) } } };
  }
};
const both = async (tool, args) => [await callServer(tool, args), await localCall(tool, args)];

const uploadServer = async (bytes) => {
  const res = await nativeFetch(`${BASE}/api/blob?doc=${SERVER_DOC}&token=${TOKEN}`,
    { method: "POST", headers: { "content-type": "application/octet-stream" }, body: bytes });
  const reply = await res.json().catch(() => null);
  return reply && (reply.blob_hash || reply.hash || reply.blob) || null;
};
const uploadLocal = async (bytes) => {
  const res = await localApi(new Request(`https://x/api/blob?doc=${LOCAL_DOC}`, {
    method: "POST", headers: { "content-type": "application/octet-stream" }, body: bytes }));
  const reply = await res.json().catch(() => null);
  return reply && (reply.blob_hash || reply.hash) || null;
};
const detailOf = (res) => (res.body && res.body.context && res.body.context.detail) ?? null;

function compareError(label, srv, loc, status, code, expectDetail) {
  if (!srv.body || !loc.body) { check(false, `${label}：有一边读不出 JSON ✗`); return; }
  check(srv.status === status,
    `${label}：服务端状态码应为 ${status} ✗ 实测 ${srv.status}｜${show(srv.body)} ⇒ 权威口径变了，要改判据`);
  check(loc.status === status, `${label}：本地状态码应为 ${status} ✗ 实测 ${loc.status}｜${show(loc.body)}`);
  check(srv.body.error_code === code, `${label}：服务端错误码应为 ${code} ✗ 实测 ${srv.body.error_code}`);
  check(loc.body.error_code === code, `${label}：本地错误码应为 ${code} ✗ 实测 ${loc.body.error_code}`);
  const sd = detailOf(srv), ld = detailOf(loc);
  check(typeof ld === "string" && ld.length > 0, `${label}：本地没给 context.detail ⇒ 调用方读不到原因 ✗`);
  check(sd === ld, `${label}：错误文案不同 ✗\n     服务端 ${sd}\n     本地     ${ld}`);
  if (expectDetail !== undefined) {
    check(sd === expectDetail,
      `${label}：服务端文案应为 ${expectDetail} ✗ 实测 ${sd} ⇒ 权威口径变了，要改判据`);
    check(ld === expectDetail, `${label}：本地文案应为 ${expectDetail} ✗ 实测 ${ld}`);
  }
}

// ══ PSD 构造器（逐字照 `psd.rs:209 encode_psd` ✓ ⇒ 只为造输入，不是被测实现）══
function packbitsEncode(row) {
  const out = [];
  let at = 0;
  while (at < row.length) {
    let run = 1;
    while (at + run < row.length && row[at + run] === row[at] && run < 128) run += 1;
    if (run >= 3) {
      out.push(((1 - run) & 0xff), row[at]);
      at += run;
      continue;
    }
    const start = at;
    while (at < row.length && at - start < 128) {
      let ahead = 1;
      while (at + ahead < row.length && row[at + ahead] === row[at] && ahead < 3) ahead += 1;
      if (ahead >= 3 && at > start) break;
      at += 1;
    }
    const count = at - start;
    out.push(count - 1);
    for (let i = start; i < at; i += 1) out.push(row[i]);
  }
  return out;
}
/** `encode_psd(width,height,rgba,channels,compression)` ✓**（**RGBA 交织输入 ⇒ 逐通道平面 ✓**）。 */
function encodePsd(width, height, rgba, channels, compression) {
  const u16 = (v) => [(v >> 8) & 0xff, v & 0xff];
  const u32 = (v) => [(v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff];
  const out = [...[0x38, 0x42, 0x50, 0x53], ...u16(1), 0, 0, 0, 0, 0, 0,
    ...u16(channels), ...u32(height), ...u32(width), ...u16(8), ...u16(3),
    ...u32(0), ...u32(0), ...u32(0), ...u16(compression)];
  const count = width * height;
  const planes = [];
  for (let c = 0; c < channels; c += 1) {
    const plane = new Uint8Array(count);
    for (let i = 0; i < count; i += 1) plane[i] = rgba[i * 4 + Math.min(c, 3)];
    planes.push(plane);
  }
  if (compression === 0) {
    for (const plane of planes) out.push(...plane);
  } else {
    const rows = [];
    for (const plane of planes) {
      for (let r = 0; r < height; r += 1) {
        rows.push(packbitsEncode(Array.from(plane.subarray(r * width, (r + 1) * width))));
      }
    }
    for (const row of rows) out.push(...u16(row.length));
    for (const row of rows) out.push(...row);
  }
  return new Uint8Array(out);
}

// ══ §A 上传口径：同一份字节 ⇒ 两边同一个 sha256 ══
const fixtureBytes = new Uint8Array(fs.readFileSync(FIXTURE));
const fxServerHash = await uploadServer(fixtureBytes);
const fxLocalHash = await uploadLocal(fixtureBytes);
if (!fxServerHash || !fxLocalHash) {
  console.log(`⊘ 不可判：上传夹具失败（服务端 ${fxServerHash}｜本地 ${fxLocalHash}）⇒ 前提不成立`);
  process.exit(3);
}
check(fxServerHash === fxLocalHash,
  `§A 上传哈希不一致 ✗ 服务端 ${fxServerHash}｜本地 ${fxLocalHash} ⇒ 两边 sha256 口径不同，本判据的相等恒等式失去意义`);

// ══ §B 真夹具的成功路径 ══
// **∴ 为什么比原子净荷而不是回执 ✗**：服务端 `import_psd` 的回执顶层**没有**
//   `blob_hash`／`object_id`／`layer_id`（**实测 keys ✓** ⇒ 它们是 `finish_mutation` 信封的字段
//   ＋ `source`）⇒ **权威形状是 `import_image` 原子的 payload** ✓（**`/api/atoms` 两边都读得到 ✓**）。
const storeLib = await import(pathToFileURL(process.cwd() + "/web/store.js").href);
const dbHandle = await storeLib.open();
const sha256Of = (bytes) => "sha256:" + crypto.createHash("sha256").update(Uint8Array.from(bytes)).digest("hex");
const readAtoms = async (where) => {
  const body = where === "srv"
    ? await nativeFetch(`${BASE}/api/atoms?doc=${encodeURIComponent(SERVER_DOC)}&since=0&token=${TOKEN}`)
        .then((r) => r.json())
    : await localApi(new Request(`https://x/api/atoms?doc=${LOCAL_DOC}`)).then((r) => r.json().catch(() => null));
  return (body && Array.isArray(body.atoms)) ? body.atoms : [];
};
const lastImportAtom = (atoms, objectId) =>
  [...atoms].reverse().find((a) => a.kind === "import_image" && a.payload && a.payload.object_id === objectId) || null;
/** **结构比较（不断字节序 ✓ 同 `tool-pwa-describe-canvas` 的口径 ✓）**。 */
function same(a, b) {
  if (a === b) return true;
  if (typeof a === "number" && typeof b === "number") {
    if (!Number.isFinite(a) || !Number.isFinite(b)) return false;
    if (Number.isInteger(a) && Number.isInteger(b)) return false;
    return Math.abs(a - b) <= 1e-12 * Math.max(1, Math.abs(a), Math.abs(b));
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => same(v, b[i]));
  }
  if (a && b && typeof a === "object" && typeof b === "object") {
    const ka = Object.keys(a).sort(), kb = Object.keys(b).sort();
    if (ka.length !== kb.length || ka.some((k, i) => k !== kb[i])) return false;
    return ka.every((k) => same(a[k], b[k]));
  }
  return false;
}
function firstDiff(a, b, p = "") {
  if (same(a, b)) return null;
  if (a && b && typeof a === "object" && typeof b === "object") {
    const keys = Array.from(new Set([...Object.keys(a), ...Object.keys(b)])).sort();
    for (const key of keys) {
      if (!(key in a)) return `${p}.${key}：服务端缺（本地 ${show(b[key])}）`;
      if (!(key in b)) return `${p}.${key}：本地缺（服务端 ${show(a[key])}）`;
      const hit = firstDiff(a[key], b[key], `${p}.${key}`);
      if (hit) return hit;
    }
    return null;
  }
  return `${p}：服务端 ${show(a)}｜本地 ${show(b)}`;
}
async function psdCase(label, bytes, args, expectSource, expectRgba) {
  const srvHash = await uploadServer(bytes);
  const locHash = await uploadLocal(bytes);
  if (!srvHash || !locHash) {
    console.log(`⊘ 不可判：${label} 上传失败（${srvHash}/${locHash}）⇒ 前提不成立`);
    process.exit(3);
  }
  check(srvHash === locHash, `${label}·§A：上传哈希不一致 ✗ 服务端 ${srvHash}｜本地 ${locHash}`);
  const [s, l] = await both("import_psd", { ...args, blob_hash: srvHash });
  check(s.status === 200 && s.body && s.body.ok === true,
    `${label}：服务端应 200 ✗ ${s.status}｜${show(s.body)}`);
  if (s.status !== 200 || !s.body || s.body.ok !== true) {
    console.log(`⊘ 不可判：${label} 服务端拒绝（${s.status} ${show(s.body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地应 200 ✗ ${l.status}｜${show(l.body)}`);
  if (!l.body || l.body.ok !== true) return null;
  // **回执里两边都有的字段逐字比 ✓**（`blob_decodable`：`x-yanshi-raw` ⇒ 恒 true ✓）：
  check(s.body.blob_decodable === true && l.body.blob_decodable === true,
    `${label}：blob_decodable 应两边为 true ✗ 服务端 ${s.body.blob_decodable}｜本地 ${l.body.blob_decodable}`);
  check(s.body.source && l.body.source && s.body.source.format === l.body.source.format
    && l.body.source.format === "psd", `${label}：source.format 应为 "psd" ✗ ${show(l.body.source)}`);
  if (expectSource) {
    check(l.body.source.width === expectSource.width && l.body.source.height === expectSource.height,
      `${label}：本地应回显 ${expectSource.width}×${expectSource.height}（判据按夹具算的期望 ✗ 不是抄服务端）✗ ${show(l.body.source)}`);
    check(s.body.source.width === expectSource.width && s.body.source.height === expectSource.height,
      `${label}：服务端应回显 ${expectSource.width}×${expectSource.height} ✗ ${show(s.body.source)}`);
    check(typeof s.body.source.note === "string" && l.body.source.note === s.body.source.note,
      `${label}：source.note 应两边逐字一致 ✗ 本地 ${show(l.body.source && l.body.source.note)}`);
  }
  // **原子净荷（权威形状 ✗ 核心恒等式在此 ✓）**：
  const sa = lastImportAtom(await readAtoms("srv"), args.object_id);
  const la = lastImportAtom(await readAtoms("loc"), args.object_id);
  check(!!sa && !!la, `${label}：两边日志里都应读得到 object_id 为 ${args.object_id} 的 import_image 原子 ✗ 服务端 ${!!sa}｜本地 ${!!la}`);
  if (!sa || !la) return null;
  // **∴ `tiles` 是服务端的分块索引 ✗ 本地没有渲染器可建 ⇒ 剥掉再逐字段比 ✓**（哨兵单独断 ✗ 不编造）
  const { tiles: srvTiles, ...srvPayload } = sa.payload;
  const { tiles: locTiles, ...locPayload } = la.payload;
  check(srvTiles === undefined || (typeof srvTiles === "string" && srvTiles.startsWith("sha256:")),
    `${label}：服务端 tiles 应是哈希或没有 ✗ ${show(srvTiles)}`);
  check(locTiles === undefined, `${label}：本地不该编造 tiles 索引 ✗ ${show(locTiles)}`);
  check(same(srvPayload, locPayload),
    `${label}：import_image 净荷不一致 ✗ 首个差异 ${firstDiff(srvPayload, locPayload)}`);
  // **核心恒等式 ✗**：合成图 `bitmap.blob_hash` 两边相等 ⇔ 解码 RGBA 逐字节相等 ✓
  check(typeof srvPayload.bitmap.blob_hash === "string"
    && srvPayload.bitmap.blob_hash === locPayload.bitmap.blob_hash,
    `${label}：合成图 blob_hash 两边不一致（⇔ 像素不同）✗ 服务端 ${srvPayload.bitmap.blob_hash}｜本地 ${locPayload.bitmap.blob_hash}`);
  // **独立期望（两边同错也红 ✓）**：期望哈希由判据**自己按夹具算**（node crypto ✓），不抄任何一方：
  const w = expectSource ? expectSource.width : 0;
  const h = expectSource ? expectSource.height : 0;
  if (expectSource) {
    for (const [who, p] of [["服务端", srvPayload], ["本地", locPayload]]) {
      check(p.bitmap.size === w * h * 4,
        `${label}（${who}）bitmap.size 应为 ${w}×${h}×4=${w * h * 4} ✗ ${p.bitmap.size}`);
      check(p.region.x === 0 && p.region.y === 0 && p.region.w === w && p.region.h === h,
        `${label}（${who}）region 应为整幅合成图 {0,0,${w},${h}} ✗ ${show(p.region)}`);
      check(p.type === "raster_patch" && p.bitmap.mime_type === "image/x-yanshi-raw",
        `${label}（${who}）type/mime 应为 raster_patch/image/x-yanshi-raw ✗ ${show(p.type)}/${show(p.bitmap.mime_type)}`);
    }
  }
  if (expectRgba) {
    const expected = sha256Of(expectRgba);
    check(locPayload.bitmap.blob_hash === expected,
      `${label}：本地哈希应等于判据独立算出的 sha256（夹具像素 ✗ 不是抄服务端）✗ 期望 ${expected}｜本地 ${locPayload.bitmap.blob_hash}`);
    check(srvPayload.bitmap.blob_hash === expected,
      `${label}：服务端哈希应等于同一独立期望 ✗ 期望 ${expected}｜服务端 ${srvPayload.bitmap.blob_hash}`);
  }
  // **本地 IndexedDB 读回（blob 先行的可验形式 ✓：原子引用必须能在本地取回同一份字节 ✗ 不是只进内核）**：
  const stored = await storeLib.getBlob(dbHandle, locPayload.bitmap.blob_hash);
  check(stored && Array.from(stored).length === locPayload.bitmap.size,
    `${label}：本地 blob 库应读回整份合成图（${locPayload.bitmap.size} 字节）✗ 实测 ${stored ? Array.from(stored).length : "无"}`);
  if (expectRgba && stored) {
    const a = Array.from(stored), b2 = Array.from(expectRgba);
    check(a.length === b2.length && a.every((v, i) => v === b2[i]),
      `${label}：本地读回的像素与夹具期望逐字节不同 ✗ 首个差异 index ${a.findIndex((v, i) => v !== b2[i])}`);
  }
  return { srv: s.body, loc: l.body, srvPayload, locPayload };
}

const rectOnLayer = { geometry: { kind: "rect", bbox: { x: 0, y: 0, w: 2, h: 2 } }, color: [0, 0, 1, 1], stroke_width: 0 };
await psdCase("§B 真夹具", fixtureBytes, { layer_id: "layer_default", object_id: "PSD_B" },
  { width: 64, height: 64 });
// 对象真的进了图层（`describe_canvas` 的计数与服务端一致 ✓）：
{
  await setupBoth("参照对象 draw_shape", "draw_shape", { layer_id: "layer_default", object_id: "REF1", data: rectOnLayer });
  const [s, l] = await both("describe_canvas", {});
  check(s.status === 200 && s.body && s.body.ok === true && l.status === 200 && l.body && l.body.ok === true,
    `§B2 describe_canvas 应两边 200 ✗ ${s.status}/${l.status}`);
  if (s.body && l.body && s.body.layers && l.body.layers) {
    const so = Object.fromEntries(s.body.layers.map((x) => [x.layer_id, x.objects]));
    const lo = Object.fromEntries(l.body.layers.map((x) => [x.layer_id, x.objects]));
    check(JSON.stringify(so) === JSON.stringify(lo), `§B2 每层对象数应一致 ✗ 服务端 ${show(so)}｜本地 ${show(lo)}`);
    check(so.layer_default === 2 && lo.layer_default === 2,
      `§B2 layer_default 上应有 2 个活对象（PSD_B＋REF1）✗ 服务端 ${so.layer_default}｜本地 ${lo.layer_default}`);
    check(s.body.masks && s.body.masks.length === 0 && l.body.masks.length === 0, "§B2 不应有蒙版 ✗");
  } else {
    check(false, `§B2 两边都该有 layers 数组 ✗ ${show(s.body && s.body.counts)}｜${show(l.body && l.body.counts)}`);
  }
}
/** **公共前序：服务端拒绝 ⇒ 前提不成立（`exit(3)`）✓；本地拒绝 ⇒ 产品失败 ✓**。 */
async function setupBoth(label, tool, args) {
  const [s, l] = await both(tool, args);
  if (s.status !== 200 || !s.body || s.body.ok !== true) {
    console.log(`⊘ 不可判：${label} 服务端拒绝（${s.status} ${show(s.body)}）⇒ 前提不成立，不是产品问题`);
    process.exit(3);
  }
  check(l.status === 200 && l.body && l.body.ok === true,
    `${label}：本地被拒 ✗ 状态 ${l.status}｜${show(l.body)}`);
  return [s, l];
}

// ══ §C RLE × 3 通道（PackBits 两条支都被同色段＋杂色逼出来 ✓）══
const CW = 40, CH = 4;
const rgbaC = [];
for (let y = 0; y < CH; y += 1) for (let x = 0; x < CW; x += 1) {
  const v = ((x / 10 | 0) + y) % 2 === 0 ? 40 : (x * 7) % 256;
  rgbaC.push(v, (v + 1) % 256, (255 - v) % 256, 255);
}
await psdCase("§C RLE×3", encodePsd(CW, CH, Uint8Array.from(rgbaC), 3, 1),
  { layer_id: "layer_default", object_id: "PSD_C" }, { width: CW, height: CH }, rgbaC);

// ══ §D RLE × 4 通道（alpha 平面逐像素 0/40/80/…：抄错平面序 ⇒ 哈希必变 ✓）══
const DW = 3, DH = 2;
const rgbaD = [];
for (let i = 0; i < DW * DH; i += 1) rgbaD.push(10, 20, 30, (i * 40) % 256);
await psdCase("§D RLE×4", encodePsd(DW, DH, Uint8Array.from(rgbaD), 4, 1),
  { layer_id: "layer_default", object_id: "PSD_D" }, { width: DW, height: DH }, rgbaD);

// ══ §E 原始 × 1 通道（g、b 复制 r，alpha=255 —— 缺省支 ✓）══
const EW = 2, EH = 2;
const rgbaE = [];
for (let i = 0; i < EW * EH; i += 1) rgbaE.push(7 * (i + 1), 99, 123, 45);
// **∴ 解码期望 ✗**：1 通道 ⇒ `r=g=b=raw[i]、a=255`（**绿蓝 alpha 是夹具的干扰项 ✗ 不该出现在结果里 ✓**）。
const expectE = [];
for (let i = 0; i < EW * EH; i += 1) { const v = 7 * (i + 1); expectE.push(v, v, v, 255); }
await psdCase("§E 原始×1", encodePsd(EW, EH, Uint8Array.from(rgbaE), 1, 0),
  { layer_id: "layer_default", object_id: "PSD_E" }, { width: EW, height: EH }, expectE);

// ══ §F 拒绝支（**每条都由夹具改几个字节造出来 ⇒ 输入确实制造出要判的情况 ✓**）══
const good = encodePsd(4, 2, Uint8Array.from(
  Array.from({ length: 8 }, (_, i) => (i * 31) % 256).flatMap((v) => [v, v, v, 255])
), 3, 0);
const patch = (bytes, at, vals) => { const c = Uint8Array.from(bytes); c.set(vals, at); return c; };
/** **RLE 长度表「说到」但行内容解码偏短 ✗**（**每行 `[0x00,0x41]` ⇒ PackBits 原样 1 字节 ✓**
 *   ⇒ 解压总长 `rows` < `pixels×channels` ⇒「合成图数据不足（解压后）」守卫的**唯一触发输入** ✓）。 */
function psdRleShortRows(width, height, channels) {
  const u16v = (v) => [(v >> 8) & 0xff, v & 0xff];
  const u32v = (v) => [(v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff, v & 0xff];
  const out = [...[0x38, 0x42, 0x50, 0x53], ...u16v(1), 0, 0, 0, 0, 0, 0,
    ...u16v(channels), ...u32v(height), ...u32v(width), ...u16v(8), ...u16v(3),
    ...u32v(0), ...u32v(0), ...u32v(0), ...u16v(1)];
  const rows = height * channels;
  for (let i = 0; i < rows; i += 1) out.push(...u16v(2));
  for (let i = 0; i < rows; i += 1) out.push(0x00, 0x41);
  return new Uint8Array(out);
}
{
  const cases = [
    ["§F1 位深 16", patch(good, 22, [0, 16]), "PSD 解析失败：位深 16 不支持（只支持 8 位；16/32 位请先导出为 8 位）"],
    ["§F2 色彩模式 4", patch(good, 24, [0, 4]), "PSD 解析失败：色彩模式 4 不支持（只支持 RGB=3；灰度/CMYK/索引色请先转换）"],
    ["§F3 版本 2", patch(good, 4, [0, 2]), "PSD 解析失败：PSD 版本 2 不支持（只支持版本 1；版本 2 是 PSB 大文档格式）"],
    // **∴ 压缩字段偏移是 38 ✗**（**26 头 ＋ 三个「4 字节长度」段（长度全 0 ⇒ 各 4 字节）= 38 ✓**）：
    //   **∴ 第 673 轮实测教训 ✗**：第一次写成 44（**照仓库夹具的 hexdump 抄的 ✗ 那夹具带图层段 ✓）
    //   ⇒ 打在像素上 ⇒ 两边都当合法原始图导入了 ✗ 这条断言差点变成安慰剂 ✓**
    ["§F4 压缩 2(ZIP)", patch(good, 38, [0, 2]), "PSD 解析失败：压缩方式 2 不支持（只支持 0=原始 与 1=RLE；ZIP 压缩是 PSB 才有的）"],
    ["§F5 截半", good.slice(0, Math.floor(good.length / 2)), null],
    ["§F6 不是 PSD", new Uint8Array([...b("not a psd at all, really not")]), null],
    ["§F7 太短", new Uint8Array([0x38, 0x42]), null],
    ["§F8 RLE 行解码偏短（解压后守卫）", psdRleShortRows(2, 2, 3), "PSD 解析失败：合成图数据不足（解压后）"],
  ];
  for (const [label, bytes, expect] of cases) {
    const h = await uploadServer(bytes);
    const hl = await uploadLocal(bytes);
    if (!h || !hl) { check(false, `${label}：上传失败 ✗ ${h}/${hl}（两边都该收任意字节当 blob ✓）`); continue; }
    const [s, l] = await both("import_psd", { layer_id: "layer_default", blob_hash: h });
    check(s.status === 400 && s.body && s.body.error_code === "invalid_argument",
      `${label}：服务端应 400 invalid_argument ✗ ${s.status}｜${show(s.body)}`);
    compareError(label, s, l, 400, "invalid_argument", expect ?? undefined);
    if (expect === null) {
      check(typeof detailOf(s) === "string" && detailOf(s).startsWith("PSD 解析失败："),
        `${label}：服务端应带前缀「PSD 解析失败：」✗ 实测 ${detailOf(s)}`);
    }
  }
  function b(s) { const a = new Uint8Array(s.length); for (let i = 0; i < s.length; i += 1) a[i] = s.charCodeAt(i); return a; }
}

// ══ §G 入口校验链（**先后顺序也是契约的一部分 ✓**：声明顺序 layer_id→blob_hash ✓）══
{
  const g1 = await both("import_psd", { blob_hash: fxServerHash });
  compareError("§G1 缺 layer_id", g1[0], g1[1], 400, "invalid_argument", "缺少必填参数 layer_id");
  const g2 = await both("import_psd", { layer_id: "layer_default" });
  compareError("§G2 缺 blob_hash", g2[0], g2[1], 400, "invalid_argument", "缺少必填参数 blob_hash");
  const g3 = await both("import_psd", { layer_id: "layer_default", blob_hash: 5 });
  compareError("§G3 blob_hash 非字符串", g3[0], g3[1], 400, "invalid_argument",
    "参数 blob_hash 类型应为 string，实际为 number");
  const g3b = await both("import_psd", { layer_id: 5, blob_hash: fxServerHash });
  compareError("§G3b layer_id 非字符串（先于 blob_hash ✓）", g3b[0], g3b[1], 400, "invalid_argument",
    "参数 layer_id 类型应为 string，实际为 number");
  const g3c = await both("import_psd", { layer_id: 5, blob_hash: "not-a-hash" });
  compareError("§G3c 两个都坏 ⇒ 按声明顺序报 layer_id", g3c[0], g3c[1], 400, "invalid_argument",
    "参数 layer_id 类型应为 string，实际为 number");
  const g4 = await both("import_psd", { layer_id: "layer_default", blob_hash: "nope" });
  compareError("§G4 非法哈希", g4[0], g4[1], 400, "invalid_argument", "blob_hash 不是合法哈希");
  // **大写十六进制 ⇒ 两边都拒 ✗**（`atom.rs:92` 要**小写**；本地旧的 `p2BlobHashOk` 收大写 ⇒ 本分支不用它 ✓）
  const upper = "sha256:" + "A".repeat(64);
  // **∴ 实测权威文案 ✗**：`write_import_psd` 把 `BlobHash::from_str` 的错误**换成了一句**
  //   「blob_hash 不是合法哈希」（**tools_psd.rs ✓ 不是内核那句「非法 blob hash: …」✗** ⇒ 判据照服务端 ✓）
  const g5 = await both("import_psd", { layer_id: "layer_default", blob_hash: upper });
  compareError("§G5 大写哈希应拒", g5[0], g5[1], 400, "invalid_argument",
    "blob_hash 不是合法哈希");
  // **不存在的合法哈希 ⇒ 404，文案是规范化后的哈希 ✗**（URL 形态喂进去也要同样只报 `sha256:…` ✓）
  const absent = "sha256:" + "0".repeat(64);
  const g6 = await both("import_psd", { layer_id: "layer_default", blob_hash: `yanshi://blob/${absent}?x=1` });
  compareError("§G6 不存在的 blob（URL 形态要规范化）", g6[0], g6[1], 404, "reference_not_found",
    `blob ${absent} 不存在`);
  const g7 = await both("import_psd", { layer_id: "layer_default", blob_hash: fxServerHash, extra: 1 });
  compareError("§G7 未知键", g7[0], g7[1], 400, "invalid_argument");
  const g8 = await both("import_psd", { layer_id: "NOPE_LAYER", blob_hash: fxServerHash, object_id: "PSD_G8" });
  if (g8[0].status === 404) {
    compareError("§G8 图层不存在", g8[0], g8[1], 404, "reference_not_found");
  } else {
    // 服务端建层是宽松的还是严格的？实测优先，不猜（本判据把权威答案记下来 ✓）。
    check(g8[0].status === 200, `§G8 服务端对未知图层的答案没被认识 ✗ ${g8[0].status}｜${show(g8[0].body)}`);
    check(g8[1].status === g8[0].status,
      `§G8 本地应与服务端同状态 ✗ 服务端 ${g8[0].status}｜本地 ${g8[1].status}｜${show(g8[1].body)}`);
  }
}

if (bad.length) {
  console.log(`✗ import_psd 本地与服务端不一致 ✗ ${bad.length} 条（另 ${okCount} 条过 ✓）`);
  for (const line of bad) console.log("   · " + line);
  process.exit(1);
}
console.log(`✓ import_psd（PSD 合成图）：与服务端同答案 ✓（${okCount} 条断言全过 ✓，合成图 sha256 逐字节一致 ✓）`);
