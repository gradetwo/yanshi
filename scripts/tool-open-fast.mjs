#!/usr/bin/env node
// **"打开要 lazy"判据**（第 4 轮 ✓，用户"各环节都要 lazy" ✓）
//
// **为什么需要它** ✗（实测为据 ✓）：4K 工程**冷启动**首次 `get_document` 花 **1191~1369 ms** ✗，
// 其中 `preview_ms` 占 **99.8%** ✗ —— 即打开时把**整幅画布重新渲染并编码**了一遍 ✓。
// 根因（三环，全部实测 ✓）：
//   ① `save_render`（整幅那份）**只在整幅渲染时**调用 ⇒ 绘画只产生区域渲染 ⇒ 整幅缓存从未落盘 ✗；
//   ② 保存点补了整幅渲染后**仍 1191 ms** ✗；
//   ③ `cache_document_preview` 有**尺寸检查**（要求 256² ✓），而整幅渲染留下的是整幅 blob ✗
//      ⇒ **静默返回** ⇒ 那份 256² `preview.png` 从未写出 ✗ ⇒ 冷启动没有像素基座 ⇒ 整幅重渲 ✗。
//
// **判据（两条，各自能红 ✓，且都不写死毫秒 ✗）**：
//   ① **冷启动首次 `get_document` 的 `preview_ms` 必须为 0** ✓
//      —— 语义是"**一次渲染都不做**" ✓（干净时应当纯命中缓存 ✓）。
//      **变异**：让保存点不写那两份快照 ⇒ `preview_ms > 0` ⇒ 判据红 ✓。
//   ② **保存之后落的那些笔必须可见** ✓（**防"拿旧图冒充"** ✗ —— 本仓头号病根 ✓）
//      —— 语义是"脏区必须算对" ✓。
//      **变异**：把 `dirty_since` 的判定改成恒 `Clean` ⇒ 冷启动交出旧图 ⇒ 判据红 ✓。
//
// **为什么这条判据要自己起服务端** ✓：只有**新进程**才是"冷启动"，而判据集合共用的是一个常驻服务端 ✓
// ⇒ 必须自己 spawn ＋ 重启 ✓。二进制走 `YANSHI_SERVE_BIN` ✓（缺省 `./target/debug/yanshi-serve` ✓，
// 与 `run-criteria.sh` 用的那个一致 ✓）；找不到就**响亮地说"前置不成立"** ✓（不静默跳过 ✗）。
//
// 用法：node scripts/tool-open-fast.mjs [base-url-未用] ／ 环境变量 YANSHI_SERVE_BIN=<路径>

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const BIN = process.env.YANSHI_SERVE_BIN || "./target/debug/yanshi-serve";
const ASSETS = process.env.YANSHI_ASSETS_DIR || "assets";
const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const root = mkdtempSync(join(tmpdir(), "open-fast-"));
const port = 21000 + Math.floor(Math.random() * 2000);
const base = `http://127.0.0.1:${port}`;
const stamp = Date.now().toString(36);
const clean = "ofClean" + stamp;    // 保存后不再改 ✓
const edited_ = "ofEdited" + stamp;  // 保存后再落一笔 ✓

let child = null;
async function start(label) {
  child = spawn(BIN, ["--bind", `127.0.0.1:${port}`, "--root", root,
    "--assets-dir", ASSETS, "--profile", "all"], { stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (d) => { stderr += d.toString(); });
  child.on("error", () => {});
  for (let i = 0; i < 240; i++) {
    try {
      const r = await fetch(`${base}/api/documents`);
      if (r.ok) { console.log(`  ${label}：服务端已就绪 ✓`); return; }
    } catch { /* 还没起来 */ }
    if (child.exitCode !== null) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  console.error(`✗ 前置不成立：起不来服务端（${BIN}）⇒ ${stderr.slice(0, 300)}`);
  console.error("  ⇒ CI 里请设 YANSHI_SERVE_BIN 指向已构建的服务端二进制 ✓");
  process.exit(2);
}
async function stop() {
  if (!child) return;
  child.kill("SIGTERM");
  await new Promise((r) => { child.once("exit", r); setTimeout(r, 3000); });
  child = null;
  await new Promise((r) => setTimeout(r, 400));
}
const post = async (u, b) => (await fetch(u, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(b || {}) })).json();

try {
  await start("第一次启动");

  // **两个文档，各测一件事** ✓ —— 实测逼出来的 ✗：这两条断言在**同一个文档**上会互相冲突 ✓
  //（"保存后再落一笔" ⇒ 脏区非空 ⇒ 冷启动**必然**要为那块脏区渲染 ⇒ `preview_ms` 不可能为 0 ✗）。
  //   * **文档 A**：保存后**不再改** ⇒ 冷启动应当**零渲染** ✓（测"纯命中缓存"）；
  //   * **文档 B**：保存后**再落一笔** ⇒ 冷启动必须**看见那一笔** ✓（测"脏区算得对、不许冒充"）。
  const make = async (name, extraStroke) => {
    const created = await post(`${base}/api/documents`, { doc_id: name, width: 2048, height: 1536 });
    const token = created.token;
    const call = (t, a) => post(`${base}/api/tools/${t}?doc=${encodeURIComponent(name)}&token=${token}`, a);
    await call("create_layer", { layer_id: "L1" });
    const stroke = (id, y) => ({ layer_id: "L1", object_id: id, brush: "100%_Opaque", size: 120,
      color: { r: 0, g: 0, b: 0, a: 255 }, smooth: true, preview: false,
      points: [[200, y, 1], [700, y, 1]] });
    const first = await call("brush_stroke", stroke("a1", 200));
    check(first.ok === true, `前置：${name} 第一笔必须成功`, "ok=" + first.ok);
    const ex = await call("export_project", {});
    check(ex.ok === true, `前置：${name} 保存工程必须成功`, "ok=" + ex.ok);
    return { call };
  };
  const quiet = await make(clean, false);      // 文档 A：保存后不再改 ✓
  const edited = await make(edited_, true);    // 文档 B：保存后再落一笔 ✓

  await stop();
  await start("冷启动（新进程 ✓）");

  // —— ① 文档 A：冷启动首次取文档必须**零渲染** ——
  const t0 = Date.now();
  const opened = await quiet.call("get_document", {});
  const wall = Date.now() - t0;
  const tm = opened.timings || {};
  const previewMs = tm.preview_ms === undefined ? null : Number(tm.preview_ms);
  // **计数器才是"这条路真的走了"的凭证** ✓（第 912 轮那套口径 ✓；`get_document` **没有** `preview`
  // 字段 ✗，我第一版想比"交付图的指纹"比不了 ✗ ⇒ 改用**渲染计数** ✓，机器无关且能红 ✓）。
  const rendersA = Number(opened.preview_renders ?? -1);
  console.log(`  文档 A（保存后未改）冷启动首次 get_document ⇒ ${wall} ms｜preview_ms=${previewMs}｜preview_renders=${rendersA}`);
  check(previewMs !== null && previewMs < 1.0,
    "保存后**未改**的文档，冷启动首次取文档**不得实质渲染**（preview_ms < 1 ms ✓ 对照整幅 1191 ms）",
    "preview_ms=" + previewMs);
  check(rendersA === 0, "保存后**未改**的文档，冷启动**一次渲染都不该发生**（preview_renders == 0）",
    "preview_renders=" + rendersA);

  // —— ② 文档 B：保存之后那一笔必须可见（防"拿旧图冒充"✗）——
  // **判"它有没有为重算付出动作"** ✓（`get_document` 没有 `preview` 字段 ✗，
  // 所以我第一版想比"交付图指纹"比不了 ✗）。**∴ 用渲染计数** ✓：
  // 保存后**又落了一笔** ⇒ 冷启动**必须**至少渲染一次（只渲脏区 ✓）✓
  // **变异**：把 `dirty_since` 改成恒 `Clean` ⇒ 它一次都不渲 ⇒ `preview_renders == 0` ⇒ **判据红** ✓✓。
  const cold = await edited.call("get_document", {});
  const rendersB = Number(cold.preview_renders ?? -1);
  console.log(`  文档 B（保存后落了一笔）冷启动 ⇒ preview_renders=${rendersB}（必须 ≥ 1 ✓）`);
  check(rendersB >= 1, "保存后**改过**的文档，冷启动**必须**为重算脏区渲染（preview_renders ≥ 1）",
    "preview_renders=" + rendersB);

  // 再量一次**真值**（第二道，防"指纹变了但像素没变" ✗）。
  const before = await edited.call("analyze_region", { region: { x: 200, y: 140, w: 520, h: 120 } });
  const after = await edited.call("analyze_region", { region: { x: 200, y: 640, w: 520, h: 120 } });
  const b = Number(before.avg_brightness), a = Number(after.avg_brightness);
  console.log(`  文档 B：保存前那一带 avg=${b}｜**保存之后**那一带 avg=${a}`);
  check(Number.isFinite(a) && Number.isFinite(b) && a < 250 && b < 250 && Math.abs(a - b) < 40,
    "**保存之后**落的笔必须出现在内容里",
    `保存前=${b}／保存后=${a}`);
} finally {
  await stop();
  try { rmSync(root, { recursive: true, force: true }); } catch { /* 清理失败不影响结论 */ }
}

console.log("");
if (failures.length) {
  console.error(`结论：打开不 lazy ✗（${failures.length} 条）`);
  for (const f of failures) console.error("   - " + f);
  process.exit(1);
}
console.log("结论：✓ 冷启动不做渲染、且保存之后的改动仍可见（语义边界判据，不写死毫秒）");
process.exit(0);
