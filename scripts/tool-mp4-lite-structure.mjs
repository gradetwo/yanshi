#!/usr/bin/env node
/**
 * **★ 极简 MP4 封装器的**结构判据**✗ ★**（**第 249 轮 ✓）
 *
 * **∴ 为什么要有它 ✗**：`crates/yanshi-http/assets/mp4-lite.js` 是**手写的盒子格式** ✓
 *   ＋ **∴ 而**手写二进制格式**最容易犯的错**是**长度／偏移差几个字节**✗
 *     ⇒ **∴ 那种错**不会崩**✗ 只会**文件打不开** ✓ ⇒ **∴ 所以**必须有一条**语义判据** ✓ ★★
 *
 * **∴ 判什么（**三件 ✗ 都能红 ✓）★**：
 *   **∴ ① 盒子树 ✗**：`ftyp` 在最前 ✗ `moov` 在 `mdat` 之前（**fastStart ✓）✗
 *     `moov > trak > mdia > minf > stbl` 里有 `stsd(avc1(avcC))`／`stts`／`stsc`／`stsz`／
 *     `stco`／`stss` ✓；
 *   **∴ ② `stsz` 的样本长度**必须等于我们喂进去的** ✓；
 *   **∴ ③ `stco` 的每个偏移**必须落在 `mdat` 里 ✗ 而且**按它取出来的字节**必须**逐字节等于**
 *     我们喂进去的那一帧** ✓ ✓（**∴ 这才是"文件真的能被解出来"的结构性证据 ✓）★★
 *
 * **∴ 变异（**能红的根据 ✓）✗**：**把 `stco` 的回填去掉（**留 0 ✓）⇒ ③ 必红** ✓
 *   ＋ **∴ 或**把 `mdat` 的长度写错 ⇒ ③ 红** ✓
 *
 * **∴ 退出码（**与全仓判据同一约定 ✓）✗**：**0 ＝ 通过｜1 ＝ 失败｜3 ＝ 判不了**（**∴ 比如
 *   `mp4-lite.js` 不存在或取不到那个全局 ★）** ✓
 */
import { existsSync, readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const MUXER = resolve(ROOT, "crates/yanshi-http/assets/mp4-lite.js");

const failures = [];
const check = (ok, label, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} ${label}${detail ? `（${detail}）` : ""}`);
  if (!ok) failures.push(label);
};

if (!existsSync(MUXER)) {
  console.error(`❌ 判不了：找不到 ${MUXER} ⇒ **∴ 这不是产品失败** ✗ 退出码 3`);
  process.exit(3);
}
await import(pathToFileURL(MUXER).href);
const YanshiMp4 = globalThis.YanshiMp4;
if (typeof YanshiMp4 !== "function") {
  console.error("❌ 判不了：`mp4-lite.js` 没有在 `globalThis` 上挂 `YanshiMp4` ⇒ 退出码 3");
  process.exit(3);
}
console.log(`  加载 ${MUXER.replace(`${ROOT}/`, "")} ⇒ YanshiMp4 ✓`);

/** **一小段**假的 avcC**✗**（**∴ 结构判据不校验 SPS/PPS 内容 ✗ 只看盒子 ✓）★ */
const avcC = Uint8Array.from([0x01, 0x64, 0x00, 0x28, 0xff, 0xe1, 0x00, 0x04, 0x67, 0x64, 0x00, 0x28]);
const frames = [
  { bytes: Uint8Array.from([1, 2, 3, 4, 5]), ticks: 3000, key: true },
  { bytes: Uint8Array.from([9, 8, 7]), ticks: 3000, key: false },
  { bytes: Uint8Array.from([6, 5, 4, 3]), ticks: 3000, key: false },
];

const muxer = new YanshiMp4({ width: 64, height: 48, timescale: 90000, description: avcC });
for (const f of frames) muxer.addSample(f.bytes, f.ticks, f.key);
const mp4 = muxer.finish();
check(mp4 instanceof Uint8Array && mp4.length > 0, "产物是 Uint8Array 且非空", `${mp4.length} 字节`);

/** **把顶层盒子走一遍 ✗**（**∴ 只到需要的那几层 ✓）★ */
const boxes = (buf, start = 0, end = buf.length) => {
  const out = [];
  let at = start;
  while (at + 8 <= end) {
    const size = (buf[at] << 24) | (buf[at + 1] << 16) | (buf[at + 2] << 8) | buf[at + 3];
    const type = String.fromCharCode(buf[at + 4], buf[at + 5], buf[at + 6], buf[at + 7]);
    if (size < 8 || at + size > end) {
      out.push({ type, size, at, bad: true });
      break;
    }
    out.push({ type, size, at, body: at + 8, end: at + size });
    at += size;
  }
  return out;
};
const find = (list, type) => list.find((b) => b.type === type);
const u32 = (buf, at) => ((buf[at] << 24) | (buf[at + 1] << 16) | (buf[at + 2] << 8) | buf[at + 3]) >>> 0;

const top = boxes(mp4);
const names = top.map((b) => b.type);
check(names[0] === "ftyp", "第一个盒子是 ftyp", names.join(","));
console.log(`  顶层盒子：${names.join(" → ")}`);
check(top.every((b) => !b.bad), "每个顶层盒子的长度都自洽（不越界）");
check(
  names.indexOf("moov") >= 0 && names.indexOf("mdat") >= 0 && names.indexOf("moov") < names.indexOf("mdat"),
  "moov 在 mdat 之前（fastStart ⇒ 边下边播）",
);

const moov = find(top, "moov");
const trak = moov ? find(boxes(mp4, moov.body, moov.end), "trak") : null;
const mdia = trak ? find(boxes(mp4, trak.body, trak.end), "mdia") : null;
const minf = mdia ? find(boxes(mp4, mdia.body, mdia.end), "minf") : null;
const stbl = minf ? find(boxes(mp4, minf.body, minf.end), "stbl") : null;
check(Boolean(stbl), "moov > trak > mdia > minf > stbl 都在");
if (!stbl) {
  console.error("❌ 盒子树不完整 ⇒ 后面的检查无法进行");
  process.exit(1);
}
const stblBoxes = boxes(mp4, stbl.body, stbl.end);
for (const need of ["stsd", "stts", "stsc", "stsz", "stco", "stss"]) {
  check(Boolean(find(stblBoxes, need)), `stbl 里有 ${need}`);
}

// **∴ stsd > avc1 > avcC ✗**（**∴ 没有 avcC ⇒ 解码器不知道 SPS/PPS ⇒ 打不开 ✓）★
const stsd = find(stblBoxes, "stsd");
const avc1 = stsd ? find(boxes(mp4, stsd.body + 8, stsd.end), "avc1") : null;
const avcCBox = avc1 ? find(boxes(mp4, avc1.body + 78, avc1.end), "avcC") : null;
check(Boolean(avcCBox), "stsd > avc1 > avcC 都在（SPS/PPS 有地方放）");
if (avcCBox) {
  const body = mp4.slice(avcCBox.body, avcCBox.body + avcC.length);
  check(
    body.length === avcC.length && body.every((b, i) => b === avcC[i]),
    "avcC 的内容逐字节等于喂进去的 description",
  );
}

// **∴ ② stsz 的样本长度 ✗** ✓
const stsz = find(stblBoxes, "stsz");
if (stsz) {
  const count = u32(mp4, stsz.body + 8);
  check(count === frames.length, `stsz 的样本数 = ${frames.length}`, String(count));
  let ok = true;
  for (let i = 0; i < frames.length; i++) {
    const size = u32(mp4, stsz.body + 12 + i * 4);
    if (size !== frames[i].bytes.length) ok = false;
  }
  check(ok, "stsz 里每个样本长度都等于喂进去的字节数");
}

// **∴ ③ stco 的偏移 ⇒ 取出来的字节必须**逐字节等于**那一帧 ✗ ★** ✓
const mdat = find(top, "mdat");
const stco = find(stblBoxes, "stco");
if (mdat && stco) {
  const count = u32(mp4, stco.body + 4);
  check(count === frames.length, `stco 的条目数 = ${frames.length}`, String(count));
  let allInside = true;
  let allEqual = true;
  const seen = [];
  for (let i = 0; i < frames.length; i++) {
    const off = u32(mp4, stco.body + 8 + i * 4);
    seen.push(off);
    const inside = off >= mdat.body && off + frames[i].bytes.length <= mdat.end;
    if (!inside) allInside = false;
    if (inside) {
      const got = mp4.slice(off, off + frames[i].bytes.length);
      if (!(got.length === frames[i].bytes.length && got.every((b, k) => b === frames[i].bytes[k]))) {
        allEqual = false;
      }
    }
  }
  check(allInside, "stco 的每个偏移都落在 mdat 之内", `偏移 ${seen.join(",")}`);
  check(allEqual, "★ 按 stco 取出来的字节逐字节等于喂进去的那一帧 ★");
} else {
  check(false, "能找到 mdat 与 stco 来做偏移核对");
}

// **∴ stss 只列关键帧 ✗** ✓
const stss = find(stblBoxes, "stss");
if (stss) {
  const count = u32(mp4, stss.body + 4);
  const first = u32(mp4, stss.body + 8);
  check(count === 1 && first === 1, "stss 只列出第 1 帧（我们只标了它是关键帧）", `${count} 条、首=${first}`);
}

console.log("");
if (failures.length) {
  console.error(`❌ ${failures.length} 项没通过：${failures.join("｜")}`);
  process.exit(1);
}
console.log("✓ MP4 盒子树、样本表与「按偏移取字节」三件都对 ⇒ 结构上可解");
