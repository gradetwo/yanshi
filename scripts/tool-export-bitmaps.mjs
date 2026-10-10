#!/usr/bin/env node
// **★ 导出工程包必须带上**不可重放的位图** ✗ ★**（第 225 轮 ✓；**规格先行 ⇒ 预期红 ✓）。
//
// **∴ 为什么 ✗**：**实测**（**第 221／224 轮 ✓）**✗**：
//   **∴ `export_project(include_bitmaps: true)`**✗
//     ⇒ **∴ 产物包**14 KiB**✗，`blobs/` **条目数 ＝ **0**** ✓
//   **∴ 而**那个位图是**不可重放**的**✗（**`image/x-yanshi-raw` ✓，**没有笔刷配方 ✓）
//     ⇒ **∴ 按实现自己的注释**✗**：
//       「**只省掉**能证明重放得出来**的位图**」（**`service.rs:2640` ✓）
//       ⇒ **★ 所以**不可重放的**必须**照装** ✓ ★**** ✓✓
//
// **∴ 已定位的机制 ✗**：**省略分支**被 `if !include_bitmaps` **守住**✗（`service.rs:2642` ✓）
//   ⇒ **∴ 我传 `true` 时**它**不该**省略** ✓
//     ⇒ **∴ 而**包里**没有 blob**✗
//       ⇒ **★ 所以真正的缺口在「**被引用的 blob**」的**收集**环节** ✓
//         （**`refs_by_blob` **是空的** ⇒ **∴ 逐行解析时**没读到引用 ✓）★**** ✓✓
//
// **∴ 判据 ✗**：**造一个**不可重放的位图**✗（**raw ＋ 无配方 ✓）
//   ⇒ **∴ `export_project(include_bitmaps: true)`**✗
//   ⇒ **∴ 断言**：**包里**至少有一个 blob**✗ ＋ **它的哈希 ＝ 那个位图的哈希** ✓**** ✓✓
//
// **∴ 变异点 ✗**：**把 `include_bitmaps` 改成 `false`** ⇒ **∴ 判据必红** ✓
//
// 用法：node scripts/tool-export-bitmaps.mjs [--rounds 1]

import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, statSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const pick = () => {
  const c = [];
  for (const root of [process.env.CARGO_TARGET_DIR, "target"].filter(Boolean)) {
    for (const k of ["release", "debug"]) {
      const p = join(root, k, "yanshi-serve");
      if (existsSync(p)) c.push(p);
    }
  }
  c.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return c[0];
};
const BIN = pick();
// **★ 打印选中的二进制 ✗ ★**（第 228 轮 ✓；**∴ 我踩过**旧二进制静默顶替** ✓）
//   **∴ 为什么 ✗**：**第 173 轮实测**：**旧的 `target/release` **静默顶替**了新构建** ✗
//     ⇒ **∴ 于是**：**判据**跑在旧代码上** ⇒ **∴ 修了也**不转绿** ✓**** ✓✓
console.log(`  二进制：${BIN}（mtime ${statSync(BIN).mtime.toISOString()}）`);
const port = 18700 + Math.floor(Math.random() * 120);
const ROOT = mkdtempSync(join(tmpdir(), "expbm-"));
const base = `http://127.0.0.1:${port}`;
const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const doc = "expbm_doc";
  const token = (await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 512, height: 512 }) })).json()).token;
  const call = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  await call("create_layer", { layer_id: "L", name: "L" });
  // **∴ 不可重放的位图 ✗**：raw ＋ **没有** `source` 配方 ✓
  const W = 64, H = 64;
  const px = Buffer.alloc(W * H * 4);
  for (let i = 0; i < W * H; i += 1) { px[i*4] = 200; px[i*4+1] = 40; px[i*4+2] = 90; px[i*4+3] = 255; }
  const up = await (await fetch(`${base}/api/blob?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: px })).json();
  const imp = await call("import_image", { layer_id: "L",
    bitmap: { blob_hash: up.blob_hash, size: up.size, mime_type: up.mime_type },
    region: { x: 0, y: 0, w: W, h: H } });
  if (imp.ok !== true) { console.error(`  ✗ 前置不成立：import_image ⇒ ${JSON.stringify(imp).slice(0, 160)}`); process.exit(2); }
  console.log(`  位图已导入：${up.blob_hash.slice(0, 22)}…（不可重放：raw ＋ 无配方）`);

  const path = join(ROOT, "expbm_pack.yanshi");
  const ex = await call("export_project", { path, include_bitmaps: true });
  if (ex.ok !== true) { console.error(`  ✗ 前置不成立：export_project ⇒ ${JSON.stringify(ex).slice(0, 200)}`); process.exit(2); }
  console.log(`  已导出 ⇒ ${path}`);

  // **∴ 查包里的 blobs/ 条目 ✓**
  const list = spawnSync("tar", ["-tzf", path], { encoding: "utf8" }).stdout || "";
  // **★ 路径匹配要**容忍前缀** ✗ ★**（第 229 轮 ✓；**∴ 判据自己的 bug ✓）：
  //   **∴ 症状 ✗**：**`tar -tzf` **的输出可能带 `./` 前缀**✗
  //     ⇒ **∴ 于是** `startsWith("blobs/")` **匹配不到** ✓
  //       ⇒ **∴ 而**产品**其实已经把位图装进去了** ✓
  //         （**实测 `BUILD-INFO`：`blobs: 1`、`blob_bytes_plain: 16384` ✓）
  //           ⇒ **★ 所以**：**拿 `includes` 代替 `startsWith`** ✓ ★**** ✓✓
  // **★ 不要用 `tar -tzf` 的**列表**数条目** ✗ ★**（第 230 轮 ✓；**∴ 判据自己的 bug ✓）
  //   **∴ 症状 ✗**：**实测**（**第 230 轮 ✓）**✗**：
  //     **∴ `tar -tzf` **输出为空**** ✗
  //       ⇒ **∴ 而**同一次** `tar -xOf … BUILD-INFO` **成功** ✓（**`blobs: 1` ✓）
  //         ⇒ **∴ 所以**：**包**存在且可读**✗ ⇒ **∴ 而**「**列表**」这条路**在这台机器上**不可靠** ✓
  //   **∴ 改法 ✗**：**直接**尝试提取那个 blob**✗
  //     ⇒ **∴ 成功** ⇒ **∴ 它**真的在包里** ✓ ★**** ✓✓
  const blobEntries = list.split("\n").filter((l) => l.includes("blobs/"));
  // **∴ 逐条列出（**供人看 ✓）**
  console.log("  ── tar 清单（前 24 行）──");
  console.log(`  包内 blobs/ 条目数 ＝ ${blobEntries.length}`);
  // **★ 打印**完整清单** ✗ ★**（第 230 轮 ✓；**∴ 保留现场 ✓）：
  //   **∴ 为什么 ✗**：**实测**（**第 229 轮 ✓）**✗**：
  //     **∴ `BUILD-INFO` **写明** `blobs: 1`**✗（**∴ 产品**已经装进去了 ✓）
  //       ⇒ **∴ 而**判据**数出 0 条** ✓
  //         ⇒ **∴ 所以**：**必须**看见包里**到底有什么**✗（**∴ 不再**靠计数猜** ✓）**** ✓✓
  //   **∴ 且**：**留一份副本**✗ ⇒ **∴ 于是**：**失败现场**不随临时目录消失** ✓**** ✓✓
  console.log("  ── tar 清单（前 24 行）──");
  for (const l of list.split("\n").slice(0, 24)) { if (l.trim()) console.log(`    ${l}`); }
  // **★ 决定性检查：**按路径提取** ✗ ★**（**∴ 不依赖列表 ✓）
  const wantSuffix0 = up.blob_hash.replace(/^sha256:/, "");
  const memberPath = `blobs/sha256/${wantSuffix0.slice(0, 2)}/${wantSuffix0.slice(2, 4)}/${wantSuffix0}`;
  const extracted = spawnSync("tar", ["-xOf", path, memberPath], { encoding: "buffer", maxBuffer: 1 << 28 });
  const extractOk = extracted.status === 0 && (extracted.stdout || Buffer.alloc(0)).length > 0;
  console.log(`  按路径提取 ${memberPath.slice(0, 40)}… ⇒ ${extractOk ? "成功" : "失败"}（${(extracted.stdout || Buffer.alloc(0)).length} B）`);
  console.log("  ── BUILD-INFO ──");
  const bi = spawnSync("tar", ["-xOf", path, "BUILD-INFO"], { encoding: "utf8" }).stdout || "";
  for (const l of bi.split("\n").filter((x) => /blob|bitmap/i.test(x))) console.log(`    ${l}`);
  check(blobEntries.length > 0 || extractOk,
    "导出 `include_bitmaps: true` 时，包内必须有 blobs/（不可重放的位图必须照装）",
    `列表 ${blobEntries.length} 条｜按路径提取 ${extractOk ? "成功" : "失败"}`);

  // **∴ 且**那个 blob **的哈希要**出现在包里** ✓**
  const wantSuffix = up.blob_hash.replace(/^sha256:/, "");
  const hit = blobEntries.some((e) => e.includes(wantSuffix.slice(0, 8)));
  check(hit || extractOk, "包内必须**包含那个不可重放位图**的 blob", `找 ${wantSuffix.slice(0, 12)}…`);
} catch (error) {
  console.error(`  ✗ 本跑没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  proc.kill("SIGKILL");
  process.exit(2);
}
proc.kill("SIGKILL");
// **★ 保留现场** ✗ ★**（第 230 轮 ✓）：**∴ 判据失败时**不要删掉临时目录** ✗
//   ⇒ **∴ 于是**：**人**可以**自己去看那个包** ✓
if (failed === 0) {
  rmSync(ROOT, { recursive: true, force: true });
} else {
  console.log(`  （现场保留：${ROOT}）`);
}
console.log("");
console.log(failed === 0 ? "  结论：导出带位图 ✓" : `  结论：导出**丢位图** ✗（${failed} 条）`);
process.exit(failed === 0 ? 0 : 1);
