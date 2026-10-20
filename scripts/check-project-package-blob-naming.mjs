#!/usr/bin/env node
// **工程包里 blob 的命名必须**对明文**算哈希**（第 1344 轮；外部「白帧」换来的）
//
// **⇒ 为什么有这条** ✗：
//   外部（apple2011）自己拼包 ⇒ 用**压缩后**的哈希命名 blob ⇒ 解压后核对不符
//   ⇒ blob 被记到**错误的键** ⇒ 原子引用的哈希查不到 ⇒ `import_image` 不画 ⇒ **全白帧**
//   （实测 1612 帧均值 255.0）
//
// **⇒ 断什么**（读**本仓自己导出**的包，不自己拼 ⇒ 保护的是**导出器的行为**）：
//   ① `blobs.encoding` 必须存在且恰为 `zlib`
//   ② 每个 blob 的**文件名**必须 == `sha256(zlib 解压后的字节)`
//   ③ 每个 blob 的**内容**必须 != 明文（即**确实压缩了**；否则 `encoding: zlib` 是假的）
//
// **⇒ 退出码**：0 通过｜1 不符｜**3 不可判**
import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import zlib from "node:zlib";
import path from "node:path";

const [base] = process.argv.slice(2);
if (!base) {
  console.error("用法: node scripts/check-project-package-blob-naming.mjs <base>");
  process.exit(3);
}
const home = process.env.YANSHI_WORKSPACE || process.env.YANSHI_EXPORT_DIR;
if (!home) {
  console.error("  ⊘ 不可判：找不到工作区（YANSHI_WORKSPACE / YANSHI_EXPORT_DIR）");
  process.exit(3);
}
const expDir = process.env.YANSHI_EXPORT_DIR || path.join(home, "exports");
const stamp = Date.now();
const post = async (p, b, doc, tok) => {
  const u = `${base}${p}${doc ? `?doc=${doc}&token=${tok}` : ""}`;
  const r = await fetch(u, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b ?? {}) });
  return r.json();
};
const sha = (buf) => createHash("sha256").update(buf).digest("hex");

// **⇒ 造一个 8×8 的 PNG**（手写，不依赖外部库 ✓）
const w = 8; const h = 8;
const rawRows = Buffer.concat(Array.from({ length: h }, () => Buffer.concat([Buffer.from([0]), Buffer.from(Array.from({ length: w }, () => [200, 60, 60, 255]).flat())])));
const png = (() => {
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4); crc.writeUInt32BE(zlib.crc32 ? zlib.crc32(td) : 0);
    return Buffer.concat([len, td, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8; ihdr[9] = 6; // 8-bit RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(rawRows)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
})();
// **⇒ 写进工作区**（`put_blob` 只读工作区内 ✓）
const imgPath = path.join(home, `pkg_probe_${stamp}.png`);
(await import("node:fs")).writeFileSync(imgPath, png);

const doc = `pkg_${stamp}`;
const d = await post("/api/documents", { doc_id: doc, width: 64, height: 64 });
if (!d?.token) { console.error("  ⊘ 不可判：建文档失败"); process.exit(3); }
const T = d.token;
await post("/api/tools/create_layer", { layer_id: "L", name: "L" }, doc, T);
const put = await post("/api/tools/put_blob", { path: imgPath, mime_type: "image/png" }, doc, T);
const hb = put?.blob_hash || put?.context?.hash;
if (put?.ok !== true || !hb) {
  console.error(`  ⊘ 不可判：put_blob 失败 ⇒ ${JSON.stringify(put).slice(0, 120)}`);
  process.exit(3);
}
await post("/api/tools/import_image", { layer_id: "L", bitmap: { blob_hash: hb, mime_type: "image/png" }, region: { x: 10, y: 10, w: 8, h: 8 } }, doc, T);
const rel = `pkg_${stamp}.yanshi`;
const ex = await post("/api/tools/export_project", { path: rel }, doc, T);
if (ex?.ok !== true) { console.error(`  ⊘ 不可判：export_project 失败 ⇒ ${JSON.stringify(ex).slice(0, 120)}`); process.exit(3); }
const pkg = path.join(expDir, rel);
if (!existsSync(pkg)) { console.error(`  ⊘ 不可判：找不到导出的包 ${pkg}`); process.exit(3); }

// **⇒ 读 tar**：`tar` 在 CI 与开发机都有（timelapse 也依赖同一套）✓
const list = execFileSync("tar", ["tf", pkg], { encoding: "utf8" }).split("\n").filter(Boolean);
const fails = [];
const ok = (m) => console.log(`  ✓ ${m}`);
const bad = (m) => { fails.push(m); console.log(`  ✗ ${m}`); };

const enc = list.find((p) => p === "blobs.encoding");
if (!enc) bad("① 包里必须有 blobs.encoding（有 blob 时）");
else {
  const v = execFileSync("tar", ["xOf", pkg, "blobs.encoding"], { encoding: "utf8" }).trim();
  if (v === "zlib") ok("① blobs.encoding = zlib");
  else bad(`① blobs.encoding 必须是 zlib ⇒ 实测「${v}」`);
}

const blobs = list.filter((p) => p.startsWith("blobs/sha256/"));
if (blobs.length === 0) bad("② 包里应当至少有一个 blob（这个用例放了一张图）");
else {
  let checkedOk = 0; const wrong = [];
  for (const b of blobs) {
    const bytes = execFileSync("tar", ["xOf", pkg, b], { maxBuffer: 1 << 26 });
    const name = path.basename(b);
    let plain;
    try { plain = zlib.inflateSync(bytes); } catch { wrong.push(`${name}（不是 zlib）`); continue; }
    const hp = sha(plain);
    if (name === hp) checkedOk += 1; else wrong.push(`${name}≠${hp.slice(0, 12)}…`);
    // ③ 内容必须真是压缩的
    if (Buffer.compare(bytes, plain) === 0) wrong.push(`${name}（内容与明文相同 ⇒ 没压缩）`);
  }
  if (wrong.length === 0) ok(`② 全部 ${checkedOk} 个 blob 的文件名 = sha256(明文)，且确实压缩过`);
  else bad(`② blob 命名/编码不符 ⇒ ${wrong.join("、")}`);
}

if (fails.length > 0) {
  console.log(`\n  ✗ ${fails.length} 处不符（包格式会让 import_image 找不到图 ⇒ 白帧）`);
  process.exit(1);
}
console.log("\n  ✓ 工程包的 blob 命名与编码正确（文件名 = sha256(明文)，内容 = zlib(明文)）");
