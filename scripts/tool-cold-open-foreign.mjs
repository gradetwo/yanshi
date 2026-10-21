#!/usr/bin/env node
// **冷开"外部作品"判据 ✗ ★**（2026-10-16 ✓）：把一份**别人给的** `.yanshi` 工程放进
// `--root` 的 `docs/<doc_id>/` 下，用**本仓当前二进制**起服务 ✓ ⇒ 读 `/api/documents` ✓
// ⇒ **断言 层／对象／head_seq 都在且非零** ✓（**这就是"真作品冷开"那条 ✓**）。
//
// **∴ 为什么需要它 ✗**：本会话前面那条 `tool-snapshot-reopen` 用的是**我自己造**的 ≥1000 原子工程 ✓；
//   而"真第三方作品能不能在现版本上正确冷开" ✗ 是**另一件事** ✓ —— 2026-10-16 用一份 296 MB 的
//   第三方包实测过（layers 4／objects 2751／head_seq 2774 ✓），**∴ 把它固化成判据** ✓。
//
// **用法**：node scripts/tool-cold-open-foreign.mjs <含 docs/<doc_id>/ 的 root> [期望的最小 head_seq]
// **退出码**：0 ⇒ 冷开正确 ✓；1 ⇒ **产品失败**（层/对象/seq 丢了 ✗）；3 ⇒ **判不了**
//   （没给目录／目录里没有 docs/* ／服务起不来 ✓ —— 与产品失败不同的码 ✓，本仓约定 ✓）。
import { existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { spawn } from "node:child_process";

// **★ 自备 fixture** ✓（第 1433 轮）：本判据原本**只能**靠人手给一个"外部工程目录" ⇒
//   `run-criteria-local.sh` 传的是 **URL** ⇒ `existsSync(url)` 为假 ⇒ **长期退出码 3**
//   （`❌ 判不了：没给（或不存在的）外部工程目录`）⇒ **等于从没跑过** ✗
// **⇒ ⇒ 现在**：给 URL／没给**都能跑** —— 自己造一个 workspace：起服务 ⇒ 建文档 + 笔画 ⇒ **杀掉**
//   ⇒ 于是 `docs/<id>/atoms.jsonl` 落在盘上 ⇒ **再起一个全新进程冷开它** ✓
//   ⇒ 这与"别人给的工程"**同一个考验**（盘上有、本进程没开过）✓
let root = process.argv[2];
let minSeq = 1;
const givenIsDir = root && existsSync(root) && !/^https?:\/\//.test(root);
if (givenIsDir) {
  minSeq = Number(process.argv[3] || 1);
} else {
  const { mkdtempSync, mkdirSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { trackTemp } = await import("./lib/temp-dirs.mjs");
  root = trackTemp(mkdtempSync(join(tmpdir(), "cold-foreign-")));
  mkdirSync(join(root, "docs"), { recursive: true });
  const bin0 = process.env.YANSHI_SERVE_BIN || "target/release/yanshi-serve";
  const p0 = Number(process.env.YANSHI_FOREIGN_BOOT_PORT || 8839);
  const boot = spawn(bin0, ["--bind", `127.0.0.1:${p0}`, "--root", root, "--no-wasm"], { stdio: "ignore" });
  try {
    let up = false;
    for (let i = 0; i < 80; i += 1) {
      await new Promise((r) => setTimeout(r, 250));
      try { if ((await fetch(`http://127.0.0.1:${p0}/health`)).ok) { up = true; break; } } catch { /* 还没起来 */ }
    }
    if (!up) { console.error("❌ 判不了：自备 fixture 的服务起不来 ⇒ 退出码 3"); process.exit(3); }
    const j = (p, body) => fetch(`http://127.0.0.1:${p0}${p}`, {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body),
    });
    const made = await (await j("/api/documents", { doc_id: "foreign_fixture", width: 300, height: 200 })).json();
    if (!made.token) { console.error("❌ 判不了：造 fixture 建不出文档 ⇒ 退出码 3"); process.exit(3); }
    const tok = made.token;
    await j(`/api/tools/create_layer?doc=foreign_fixture&token=${tok}`, { layer_id: "L", name: "L" });
    for (const y of [60, 100, 140]) {
      await j(`/api/tools/brush_stroke?doc=foreign_fixture&token=${tok}`, {
        layer_id: "L", brush: "calligraphy.myb", size: 16, opacity: 1.0, color: [0.1, 0.1, 0.1],
        points: [[40, y, 0.9], [260, y, 0.9]], preview: false, wait_for_render: false });
    }
    console.log("  · 自备 fixture 已建成（3 笔）⇒ 杀掉服务，让 atoms 落盘");
  } finally {
    try { boot.kill("SIGKILL"); } catch { /* 已退出 */ }
    await new Promise((r) => setTimeout(r, 800));
  }
}
const docsDir = join(root, "docs");
if (!existsSync(docsDir)) {
  console.error(`❌ 判不了：${docsDir} 下没有 docs/ ⇒ 退出码 3`);
  process.exit(3);
}
const ids = readdirSync(docsDir).filter((n) => existsSync(join(docsDir, n, "atoms.jsonl")));
if (!ids.length) {
  console.error("❌ 判不了：docs/ 下没有含 atoms.jsonl 的工程 ⇒ 退出码 3");
  process.exit(3);
}
const bin = process.env.YANSHI_SERVE_BIN || "target/release/yanshi-serve";
const port = Number(process.env.YANSHI_FOREIGN_PORT || 8840);
const srv = spawn(bin, ["--bind", `127.0.0.1:${port}`, "--root", root, "--no-wasm"], { stdio: "ignore" });
process.on("exit", () => { try { srv.kill(); } catch { /* 已退出 */ } });
const cleanup = () => { try { srv.kill(); } catch { /* 已退出 */ } };

let list = null;
for (let i = 0; i < 80; i += 1) {
  await new Promise((r) => setTimeout(r, 250));
  try {
    const res = await fetch(`http://127.0.0.1:${port}/api/documents`);
    if (res.ok) { list = await res.json(); break; }
  } catch { /* 还没起来 */ }
}
if (!list) { console.error("❌ 判不了：服务起不来／拿不到文档列表 ⇒ 退出码 3"); cleanup(); process.exit(3); }

const docs = (list.documents || []).filter((d) => ids.includes(d.doc_id));
if (!docs.length) { console.error("❌ 判不了：列表里没有这些工程 ⇒ 退出码 3"); cleanup(); process.exit(3); }
let bad = 0;
for (const d of docs) {
  const ok = (d.layers | 0) > 0 && (d.objects | 0) > 0 && (d.head_seq | 0) >= minSeq && d.persisted === true;
  console.log(`  ${ok ? "✓" : "✗"} ${d.doc_id} ⇒ 层 ${d.layers}｜对象 ${d.objects}｜head_seq ${d.head_seq}` +
    `｜${d.width}×${d.height}｜persisted=${d.persisted}`);
  if (!ok) bad += 1;
}
cleanup();
if (bad) { console.error(`❌ 有 ${bad} 份外部工程的冷开状态不对（层/对象/seq 丢了 ⇒ **这是产品失败**）`); process.exit(1); }
console.log(`✓ 外部工程冷开正确 ✓：${docs.length} 份，层/对象/head_seq 都在 ✓（≥${minSeq}）`);
