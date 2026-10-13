#!/usr/bin/env node
/**
 * **★ 极简 WebM 封装器的**结构判据**✗ ★**（**第 250 轮 ✓）
 *
 * **∴ 为什么要有它 ✗**：与 `tool-mp4-lite-structure` 同一个理由 ✓ —— `webm-lite.js`
 *   也是**手写的二进制格式** ✓ ⇒ **∴ 长度／偏移差几字节**不会崩 ✗ 只会**播不了** ✓
 *     ＋ **∴ 而这正是 CI 里唯一能走的那条路**✗（**Chrome for Testing 没有 H.264 ✓）
 *       ⇒ **∴ 它错了 ⇒ **CI 里那条功能就没了兜底** ✓ ★★
 *
 * **∴ 判什么（**四件 ✗ 都能红 ✓）★**：
 *   **∴ ① EBML 头 ✗**：**`DocType` 必须是 `webm`** ✓（**写成 `matroska` 有些播放器仍收 ✗
 *     但那不是我们要的容器 ✓）；
 *   **∴ ② 轨道 ✗**：`TrackType == 1`（视频 ✓）✗ `CodecID ∈ {V_VP8, V_VP9}` ✗
 *     `PixelWidth／PixelHeight` 等于喂进去的 ✓；
 *   **∴ ③ 时间戳 ✗**：`Info` 的 `TimestampScale == 1_000_000`（**∴ 于是**时间戳单位
 *     是**毫秒** ✓）＋ `SimpleBlock` 的**相对时间戳**要能把绝对时间**还原出来** ✓；
 *   **∴ ④ 帧字节 ✗ ★★★**：**把每个 `SimpleBlock` 里的帧字节取出来**必须**逐字节等于
 *     喂进去的那一帧** ✓ ✓（**∴ 这条才是"真能被解出来"的结构性证据 ✓）★★
 *
 * **∴ 变异（**能红的根据 ✓）✗**：**把 `SimpleBlock` 的头从 4 字节写成 3 字节**✗
 *   ⇒ **∴ ④ 必红** ✓；**把 `TrackType` 写成 2（音频 ✓）⇒ ② 必红** ✓ ★
 *
 * **∴ 退出码（**与全仓判据同一约定 ✓）✗**：**0 ＝ 通过｜1 ＝ 失败｜3 ＝ 判不了** ✓
 */
import { existsSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MUXER = resolve(ROOT, "crates/yanshi-http/assets/webm-lite.js");

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label);
};

if (!existsSync(MUXER)) {
  console.error(`❌ 判不了：找不到 ${MUXER} ⇒ 退出码 3`);
  process.exit(3);
}
await import(pathToFileURL(MUXER).href);
const YanshiWebm = globalThis.YanshiWebm;
if (typeof YanshiWebm !== "function") {
  console.error("❌ 判不了：`webm-lite.js` 没在 `globalThis` 上挂 `YanshiWebm` ⇒ 退出码 3");
  process.exit(3);
}
console.log(`  加载 ${MUXER.replace(`${ROOT}/`, "")} ⇒ YanshiWebm ✓`);

const frames = [
  { bytes: Uint8Array.from([0x10, 0x20, 0x30, 0x40, 0x50]), ts: 0, key: true },
  { bytes: Uint8Array.from([0x11, 0x22, 0x33]), ts: 33, key: false },
  { bytes: Uint8Array.from([0xaa, 0xbb, 0xcc, 0xdd]), ts: 67, key: false },
  { bytes: Uint8Array.from([0x99]), ts: 100, key: true },
];
const muxer = new YanshiWebm({ width: 64, height: 48, codec: "V_VP9", fps: 30 });
for (const f of frames) muxer.addSample(f.bytes, f.ts, f.key);
const webm = muxer.finish();
check(webm instanceof Uint8Array && webm.length > 0, "产物是 Uint8Array 且非空", `${webm.length} 字节`);

// **∴ 一个极简 EBML 读法 ✗**：**ID 原样 1–4 字节 ✓ 长度是 VINT ✓** ✓
const readVint = (buf, at) => {
  const first = buf[at];
  if (first === undefined || first === 0) return null;
  let len = 1;
  while (len <= 8 && (first & (1 << (8 - len))) === 0) len++;
  if (len > 8) return null;
  let value = first & ((1 << (8 - len)) - 1);
  for (let i = 1; i < len; i++) value = value * 256 + buf[at + i];
  return { value, len, allOnes: value === Math.pow(2, 7 * len) - 1 };
};
/** **元素列表 ✗**（**从 `start` 到 `end` ✓）★ */
const elements = (buf, start = 0, end = buf.length) => {
  const out = [];
  let at = start;
  while (at < end) {
    // **★ EBML 的 ID 长度看**首字节的前导位**✗ ★**（**第 250 轮修 ✓）：
    //   **∴ 规则 ✗**：`1xxxxxxx` ⇒ 1 字节 ✗ `01xxxxxx` ⇒ 2 ✗ `001xxxxx` ⇒ 3 ✗
    //     `0001xxxx` ⇒ 4 ✓（**∴ 与 VINT 同一套前导位 ✓）
    //   **∴ 我第一版怎么错的 ✗**：**枚举"哪些 ID 是几字节"**✗ ⇒ **∴ 一漏就**整段错位** ✓
    //     **∴ 实测后果 ✗**：`0x2AD7B1`（**3 字节 ✓）被当成 2 字节 ⇒ **`TimestampScale`
    //       **"缺失"**✗ ⇒ **∴ 判据**冤枉了产品** ✓（**`0x23E383` 同样是 3 字节 ✓）★★
    let idLen = 1;
    while (idLen <= 4 && (buf[at] & (1 << (8 - idLen))) === 0) idLen++;
    if (idLen > 4) break;
    if (at + idLen > end) break;
    const id = buf.subarray(at, at + idLen);
    const size = readVint(buf, at + idLen);
    if (!size) break;
    const body = at + idLen + size.len;
    const stop = body + size.value;
    if (stop > end) {
      out.push({ id, body, end: stop, bad: true, raw: buf.subarray(at, Math.min(end, at + 24)) });
      break;
    }
    out.push({ id, body, end: stop, size: size.value });
    at = stop;
  }
  return out;
};
const hex = (id) => Array.from(id).map((b) => b.toString(16).padStart(2, "0")).join("");
const find = (list, wanted) => list.find((b) => hex(b.id) === wanted);
const uintOf = (buf, el) => {
  let value = 0;
  for (let i = el.body; i < el.end; i++) value = value * 256 + buf[i];
  return value;
};
const textOf = (buf, el) => {
  let s = "";
  for (let i = el.body; i < el.end; i++) s += String.fromCharCode(buf[i]);
  return s;
};

const top = elements(webm);
const ebml = find(top, "1a45dfa3");
const segment = find(top, "18538067");
check(Boolean(ebml), "顶层有 EBML 头");
check(Boolean(segment), "顶层有 Segment");
if (!ebml || !segment) {
  console.error("❌ 顶层结构不完整 ⇒ 后面的检查无法进行");
  process.exit(1);
}
const headerKids = elements(webm, ebml.body, ebml.end);
const docType = find(headerKids, "4282");
check(Boolean(docType) && textOf(webm, docType) === "webm", "DocType = webm", docType ? textOf(webm, docType) : "缺失");

const segKids = elements(webm, segment.body, segment.end);
const info = find(segKids, "1549a966");
const tracks = find(segKids, "1654ae6b");
check(Boolean(info), "Segment 里有 Info");
check(Boolean(tracks), "Segment 里有 Tracks");
if (info) {
  const scale = find(elements(webm, info.body, info.end), "2ad7b1");
  check(Boolean(scale) && uintOf(webm, scale) === 1_000_000, "TimestampScale = 1000000（毫秒为单位）", scale ? String(uintOf(webm, scale)) : "缺失");
}
if (tracks) {
  const entry = find(elements(webm, tracks.body, tracks.end), "ae");
  const kids = entry ? elements(webm, entry.body, entry.end) : [];
  const type = find(kids, "83");
  const codec = find(kids, "86");
  const video = find(kids, "e0");
  check(Boolean(type) && uintOf(webm, type) === 1, "TrackType = 1（视频）", type ? String(uintOf(webm, type)) : "缺失");
  const codecId = codec ? textOf(webm, codec) : "";
  check(codecId === "V_VP8" || codecId === "V_VP9", "CodecID 是 V_VP8 或 V_VP9", codecId || "缺失");
  if (video) {
    const vkids = elements(webm, video.body, video.end);
    const w = find(vkids, "b0");
    const h = find(vkids, "ba");
    check(Boolean(w) && uintOf(webm, w) === 64, "PixelWidth = 64", w ? String(uintOf(webm, w)) : "缺失");
    check(Boolean(h) && uintOf(webm, h) === 48, "PixelHeight = 48", h ? String(uintOf(webm, h)) : "缺失");
  } else {
    check(false, "TrackEntry 里有 Video（像素尺寸）");
  }
}

// **★★★ ④ 帧字节 ✗**：**把每个 SimpleBlock 的负载切开 ✗ 逐字节比** ✓ ★★★
const clusters = segKids.filter((b) => hex(b.id) === "1f43b675");
check(clusters.length >= 1, "至少有一个 Cluster", `${clusters.length} 个`);
const seen = [];
let blocks = 0;
let allEqual = true;
let allTsOk = true;
for (const cluster of clusters) {
  const kids = elements(webm, cluster.body, cluster.end);
  const tsEl = find(kids, "e7");
  const clusterTs = tsEl ? uintOf(webm, tsEl) : -1;
  for (const block of kids.filter((b) => hex(b.id) === "a3")) {
    const payload = webm.subarray(block.body, block.end);
    if (payload.length < 4) {
      allEqual = false;
      continue;
    }
    // **∴ 头 4 字节 ✗**：`[0x81][int16 相对时间戳][flags]` ✓
    const track = payload[0];
    const relative = (payload[1] << 8) | payload[2];
    const flags = payload[3];
    const data = payload.subarray(4);
    const want = frames[blocks];
    if (!want || track !== 0x81) {
      allEqual = false;
      continue;
    }
    const absolute = clusterTs + (relative >= 0x8000 ? relative - 0x10000 : relative);
    if (absolute !== want.ts) allTsOk = false;
    if (flags & 0x80) {
      if (!want.key) allTsOk = allTsOk; // **∴ 关键帧标志只做记录 ✗ 不断言 ✓**
    }
    if (!(data.length === want.bytes.length && data.every((b, i) => b === want.bytes[i]))) {
      allEqual = false;
    }
    seen.push(absolute);
    blocks += 1;
  }
}
check(blocks === frames.length, `SimpleBlock 数量 = ${frames.length}`, String(blocks));
check(allTsOk, "每个 SimpleBlock 的相对时间戳都能还原出绝对时间戳", seen.join(","));
check(allEqual, "★ 每帧字节逐字节等于喂进去的那一帧 ★");

console.log("");
if (failures.length) {
  console.error(`❌ ${failures.length} 项没通过：${failures.join("｜")}`);
  process.exit(1);
}
console.log("✓ EBML 头、轨道、时间戳与「帧字节逐字节相等」四件都对 ⇒ 结构上可解");
