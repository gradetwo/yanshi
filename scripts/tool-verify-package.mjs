#!/usr/bin/env node
// **★ 服务端 `verify_package`（期 2 块 3，D2）** ✓
//   样本包（2 笔 import）⇒ `missing_body==0` + `missing_derived==2` + 恒等式；
//   剔一个真在用本体 ⇒ `missing_body` +1 且 `complete` 翻转；
//   朴素口径（只认 `blob_hash`/`blobs`）对照 ⇒ 必须报缺 0（证明它漏检）；
//   `check_package.py` 独立口径对照 ⇒ 两处不一致即红。
//
// **变异点**：分类器把 `tiles` 当成本体 ⇒ `missing_body` 变 2 ⇒ 红；
// 把文本口径改成只扫 `blob_hash` ⇒ `referenced` 塌掉、恒等式红 ✓。
//
// 用法：node scripts/tool-verify-package.mjs [--rounds 1]

// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, existsSync, statSync, rmSync, writeFileSync, mkdirSync, unlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { trackTemp } from "./lib/temp-dirs.mjs";

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
console.log(`  二进制：${BIN}（mtime ${statSync(BIN).mtime.toISOString()}）`);
const port = 18700 + Math.floor(Math.random() * 120);
const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "vfypkg-")));
const base = `http://127.0.0.1:${port}`;
const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
// **按值扫 64 位十六进制**（与服务端 `scan_hex64` / py 的 `HEX64` 同口径：
// 最长 `[0-9a-f]` 连续段恰好 64 位才算）。
const scanHex64 = (text) => {
  const out = new Set();
  for (const m of text.matchAll(/(?<![0-9a-f])[0-9a-f]{64}(?![0-9a-f])/g)) out.add(m[0]);
  return out;
};
const stripPrefix = (s) => (s.startsWith("sha256:") ? s.slice("sha256:".length) : s);
// **朴素口径**（py 的 C3）：只认 `blob_hash`/`hash`/`blobs` 键。
const naiveHashes = (node, out) => {
  if (Array.isArray(node)) { for (const item of node) naiveHashes(item, out); return; }
  if (node && typeof node === "object") {
    for (const [key, value] of Object.entries(node)) {
      if ((key === "blob_hash" || key === "hash") && typeof value === "string") {
        const h = stripPrefix(value);
        if (h.length === 64) out.add(h);
      } else if (key === "blobs" && Array.isArray(value)) {
        for (const item of value) {
          if (typeof item === "string") {
            const h = stripPrefix(item);
            if (h.length === 64) out.add(h);
          }
        }
      } else {
        naiveHashes(value, out);
      }
    }
  }
};
const tarExtract = (pack, member) =>
  spawnSync("tar", ["-xOf", pack, member], { encoding: "utf8", maxBuffer: 1 << 28, timeout: 90_000 }).stdout || "";
const tarList = (pack) =>
  (spawnSync("tar", ["-tf", pack], { encoding: "utf8", timeout: 90_000 }).stdout || "")
    .split("\n").map((l) => l.trim()).filter(Boolean);
// **`check_package.py` 独立口径**：有脚本才对照，没有就跳过并明说（CI 里没有
// `yanshi-recovery/`，对照只在本机跑；本体的数由 Rust 测试 + 本判据守）。
const findCheckPy = () => {
  const candidates = [
    process.env.YANSHI_CHECK_PACKAGE_PY,
    resolve("..", "yanshi-recovery", "yanshi-paint-0.3.0", "scripts", "check_package.py"),
    resolve(dirname(fileURLToPath(import.meta.url)), "..", "..", "yanshi-recovery", "yanshi-paint-0.3.0", "scripts", "check_package.py"),
  ].filter(Boolean);
  return candidates.find((p) => existsSync(p));
};
const runCheckPy = (pack) => {
  const script = findCheckPy();
  if (!script) return null;
  const r = spawnSync("python3", [script, "--quiet", pack], { encoding: "utf8", timeout: 120_000 });
  const line = (r.stdout || "").split("\n").find((l) => l.startsWith("[machine]")) || "";
  const nums = Object.fromEntries([...line.matchAll(/(\w+)=(\d+)/g)].map((m) => [m[1], Number(m[2])]));
  return { rc: r.status, ...nums };
};
try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const doc = "vfypkg_doc";
  const token = (await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 256, height: 128 }) })).json()).token;
  const call = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  await call("create_layer", { layer_id: "L", name: "L" });
  // **两笔 import，各 64×64** ⇒ 每笔一个 tile 索引（`payload.tiles`），本体照装。
  const importRaw = async (id, x, color) => {
    const W = 64, H = 64;
    const px = Buffer.alloc(W * H * 4);
    for (let i = 0; i < W * H; i += 1) { px[i * 4] = color[0]; px[i * 4 + 1] = color[1]; px[i * 4 + 2] = color[2]; px[i * 4 + 3] = 255; }
    const up = await (await fetch(`${base}/api/blob?doc=${doc}&token=${token}`, {
      method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: px })).json();
    const imp = await call("import_image", { layer_id: "L", object_id: id,
      bitmap: { blob_hash: up.blob_hash, size: up.size, mime_type: up.mime_type },
      region: { x, y: 0, w: W, h: H } });
    if (imp.ok !== true) { console.error(`  ✗ 前置不成立：import_image ⇒ ${JSON.stringify(imp).slice(0, 160)}`); process.exit(2); }
    return up.blob_hash;
  };
  const bodyA = await importRaw("img_a", 0, [200, 40, 90]);
  const bodyB = await importRaw("img_b", 64, [40, 120, 200]);
  console.log(`  两笔已导入：本体 ${stripPrefix(bodyA).slice(0, 12)}… / ${stripPrefix(bodyB).slice(0, 12)}…`);

  const pack = join(ROOT, "sample.yanshi");
  const ex = await call("export_project", { path: pack, include_bitmaps: true });
  if (ex.ok !== true) { console.error(`  ✗ 前置不成立：export_project ⇒ ${JSON.stringify(ex).slice(0, 200)}`); process.exit(2); }
  const report = await call("verify_package", { path: pack });
  if (report.ok !== true) { console.error(`  ✗ 前置不成立：verify_package ⇒ ${JSON.stringify(report).slice(0, 200)}`); process.exit(2); }
  console.log(`  体检：referenced=${report.referenced} present=${report.present} missing_body=${report.missing_body} missing_derived=${report.missing_derived} omitted=${report.omitted} complete=${report.complete}`);
  check(report.complete === true, "样本包应当完整（只缺派生）", `complete=${report.complete}`);
  check(report.missing_body === 0, "样本包 missing_body==0", `实测 ${report.missing_body}`);
  check(report.missing_derived === 2, "样本包 missing_derived==2", `实测 ${report.missing_derived}`);
  check(report.missing_total === 2, "样本包 missing_total==2", `实测 ${report.missing_total}`);
  check(report.omitted === 0, "样本包 omitted==0（include_bitmaps:true，什么都没省）", `实测 ${report.omitted}`);
  check(report.missing_body + report.missing_derived === report.missing_total,
    "恒等式 missing_body + missing_derived == missing_total", `${report.missing_body}+${report.missing_derived} vs ${report.missing_total}`);
  check(report.present_and_referenced + report.missing_total + report.omitted === report.referenced,
    "恒等式 referenced == present_and_referenced + missing_total + omitted",
    `${report.present_and_referenced}+${report.missing_total}+${report.omitted} vs ${report.referenced}`);
  check(report.present === report.present_and_referenced,
    "样本包没有孤儿 blob ⇒ referenced == present + missing_total 也成立",
    `present=${report.present} present_and_referenced=${report.present_and_referenced}`);

  // **派生的那 2 个必须恰好是 `payload.tiles`**（不是随便哪两个缺失）。
  const atomsText = tarExtract(pack, "atoms.jsonl");
  const tiles = new Set();
  for (const line of atomsText.split("\n")) {
    if (!line.trim()) continue;
    const atom = JSON.parse(line);
    if (typeof atom?.payload?.tiles === "string") tiles.add(stripPrefix(atom.payload.tiles));
  }
  check(tiles.size === 2, "两笔 import 各应有一个 tile 索引", `实测 ${tiles.size} 个`);
  check(new Set(report.missing_derived_hashes).size === 2
    && report.missing_derived_hashes.every((h) => tiles.has(h)),
    "missing_derived 必须恰好是那两个 tile 索引", `${report.missing_derived_hashes.join(",").slice(0, 40)}…`);

  // **朴素口径对照**：只认 `blob_hash`/`blobs` ⇒ 必须报缺 0（它漏掉了 tiles）。
  const members = tarList(pack);
  const present = new Set(members.filter((m) => m.includes("blobs/")).map((m) => m.split("/").pop()).filter((s) => s.length === 64));
  const naive = new Set();
  for (const line of atomsText.split("\n")) {
    if (!line.trim()) continue;
    naiveHashes(JSON.parse(line), naive);
  }
  const naiveMissing = [...naive].filter((h) => !present.has(h)).length;
  check(naiveMissing === 0, "朴素口径（只认 blob_hash/blobs）必须报缺 0", `实测缺 ${naiveMissing}（全量口径缺 ${report.missing_total}）`);

  // **`check_package.py` 独立口径对照**（有脚本才跑）。
  const py = runCheckPy(pack);
  if (py === null) {
    console.log("  ⊘ py 对照跳过：本机没有 check_package.py（CI 里没有 yanshi-recovery/，对照只在本机跑）");
  } else {
    console.log(`  py 对照：rc=${py.rc} referenced=${py.referenced} missing=${py.missing} missing_body=${py.missing_body} missing_tile=${py.missing_tile}`);
    check(py.rc === 0, "py 口径：只缺派生 ⇒ 退出 0", `rc=${py.rc}`);
    check(py.missing === report.missing_total && py.missing_body === report.missing_body && py.missing_tile === report.missing_derived,
      "py 口径三数与服务端一致（missing/missing_body/missing_tile）",
      `py ${py.missing}/${py.missing_body}/${py.missing_tile} vs 服务端 ${report.missing_total}/${report.missing_body}/${report.missing_derived}`);
  }

  // **剔一个真在用本体** ⇒ `missing_body` +1 且 `complete` 翻转。
  // **不要用 `tar --delete`** ✗（第 1132 轮整合实测）：本仓的包是**手写 tar**
  //（无目录条目）⇒ GNU tar 的 `--delete` 会把归档重排坏（实测：要删的那条还在、
  //  另一条被复制成两份）⇒ `missing_body` 永远是 0 ⇒ **判据假红** ✗。
  // ⇒ 改成「解包 → 删文件 → 重新打包」；成员列表保持原样（不带 `./` 前缀 ✓）。
  // ⇒ 并**排除 tile 索引**：只挑本体（`tiles` 集合里的不算 ✓），否则删的是派生 ⇒ 该红的是另一条 ✓。
  const bodyMember = members.find((m) => {
    if (!m.includes("blobs/")) return false;
    const h = m.split("/").pop();
    return h.length === 64 && !tiles.has(h);
  });
  if (!bodyMember) {
    console.error("  ⊘ 不可判：包内找不到本体 blob（不是产品问题）");
    process.exit(3);
  }
  const doctored = join(ROOT, "doctored.yanshi");
  const work = join(ROOT, "doctored-dir");
  mkdirSync(work, { recursive: true });
  execFileSync("tar", ["-xf", pack, "-C", work], { timeout: 90_000 });
  unlinkSync(join(work, bodyMember));
  const keep = members.filter((m) => m !== bodyMember);
  execFileSync("tar", ["-cf", doctored, "-C", work, ...keep], { timeout: 90_000 });
  const droppedHash = bodyMember.split("/").pop();
  const after = await call("verify_package", { path: doctored });
  check(after.ok === true, "篡改包仍能体检（不是读包失败）", JSON.stringify(after).slice(0, 120));
  check(after.complete === false, "剔本体后 complete 必须翻转", `complete=${after.complete}`);
  check(after.missing_body === 1, "剔本体后 missing_body==1", `实测 ${after.missing_body}`);
  check(after.missing_derived === 2, "剔本体后 missing_derived 仍为 2", `实测 ${after.missing_derived}`);
  check(after.missing_total === 3, "剔本体后 missing_total==3", `实测 ${after.missing_total}`);
  check(after.missing_body_hashes?.[0] === droppedHash, "缺的本体必须恰好是抠掉的那一个", `实测 ${after.missing_body_hashes?.[0]}`);
  const py2 = runCheckPy(doctored);
  if (py2 === null) {
    console.log("  ⊘ py 对照（篡改包）跳过：本机没有 check_package.py");
  } else {
    console.log(`  py 对照（篡改包）：rc=${py2.rc} missing=${py2.missing} missing_body=${py2.missing_body} missing_tile=${py2.missing_tile}`);
    check(py2.rc === 1, "py 口径：缺本体 ⇒ 退出 1", `rc=${py2.rc}`);
    check(py2.missing === after.missing_total && py2.missing_body === after.missing_body && py2.missing_tile === after.missing_derived,
      "py 口径三数与服务端一致（篡改包）",
      `py ${py2.missing}/${py2.missing_body}/${py2.missing_tile} vs 服务端 ${after.missing_total}/${after.missing_body}/${after.missing_derived}`);
  }
} catch (error) {
  console.error(`  ✗ 本跑没有结论：${String((error && error.message) || error).slice(0, 200)}`);
  proc.kill("SIGKILL");
  process.exit(2);
}
proc.kill("SIGKILL");
if (failed === 0) {
  rmSync(ROOT, { recursive: true, force: true });
} else {
  console.log(`  （现场保留：${ROOT}）`);
}
console.log("");
console.log(failed === 0 ? "  结论：verify_package 体检数对 ✓" : `  结论：verify_package 体检数不对 ✗（${failed} 条）`);
process.exit(failed === 0 ? 0 : 1);
