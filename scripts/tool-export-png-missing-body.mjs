#!/usr/bin/env node
// **★ `export_png` 缺本体 blob 失败（期 2 块 3，P2-11 纳入 B2）** ✓
//   import 一张 64×64 raw 位图后，直接删磁盘上的本体 blob 文件，
//   再 `export_png` ⇒ 必须失败（`reference_not_found`，点名缺失哈希），且不产图；
//   `render_region` 同一条件下仍降级出图 + 告警（渲染路本身不变）。
//
// **变异点**：去掉 `export_png` 的检查 ⇒ 本判据红（导出成功、只有告警）✓。
// 前置：若修前的导出已经失败（不是"成功 + 告警"），红的不是判据 ——
// 本判据在修前必须先看到一次成功导出（`render_region` 的告警是其凭证）。
//
// 用法：node scripts/tool-export-png-missing-body.mjs [--rounds 1]

// **前提失败必须 exit(3)**（第 1084 轮）：无服务时 fetch 抛异常 ⇒ uncaught ⇒ 退出码 1 ✗
// ⇒ 按 336 轮那条：条件不满足应 exit(3)（不可判），不是 exit(1)（产品失败）
process.on("uncaughtException", (error) => {
  console.error(`  ⊘ 判据自身出错（不是产品问题）：${String(error).slice(0, 200)}`);
  process.exit(3);
});
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, existsSync, statSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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
const ROOT = trackTemp(mkdtempSync(join(tmpdir(), "exppngmiss-")));
const base = `http://127.0.0.1:${port}`;
const proc = spawn(BIN, ["--root", ROOT, "--bind", `127.0.0.1:${port}`], { stdio: "ignore" });
process.on("exit", () => { try { proc.kill(); } catch { /* 已退出 */ } });
let failed = 0;
const check = (ok, name, detail) => {
  console.log(`  ${ok ? "✓" : "✗"} **${name}**${detail ? `（${detail}）` : ""}`);
  if (!ok) failed += 1;
};
try {
  for (let i = 0; i < 80; i += 1) { try { if ((await fetch(`${base}/health`)).ok) break; } catch {} await sleep(250); }
  const doc = "exppngmiss_doc";
  const token = (await (await fetch(`${base}/api/documents`, { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc, width: 128, height: 128 }) })).json()).token;
  const call = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) })).json();
  await call("create_layer", { layer_id: "L", name: "L" });
  // **导一张 64×64 raw 位图**（本体 blob 在磁盘 CAS 里），**不要先渲染** ——
  // 渲染会把完整像素写进区域块缓存，删 blob 后再导出就命中缓存、看不到缺失。
  const W = 64, H = 64;
  const px = Buffer.alloc(W * H * 4);
  for (let i = 0; i < W * H; i += 1) { px[i * 4] = 200; px[i * 4 + 1] = 40; px[i * 4 + 2] = 90; px[i * 4 + 3] = 255; }
    const up = await (await fetch(`${base}/api/blob?doc=${doc}&token=${token}`, {
    method: "POST", headers: { "content-type": "image/x-yanshi-raw" }, body: px })).json();
  const imp = await call("import_image", { layer_id: "L", object_id: "img_a",
    bitmap: { blob_hash: up.blob_hash, size: up.size, mime_type: up.mime_type },
    region: { x: 0, y: 0, w: W, h: H } });
  if (imp.ok !== true) { console.error(`  ✗ 前置不成立：import_image ⇒ ${JSON.stringify(imp).slice(0, 160)}`); process.exit(2); }
  const hash = up.blob_hash.replace(/^sha256:/, "");
  // **直接删磁盘上的本体 blob 文件**，造"包里引着、存储里没有"。
  const found = spawnSync("find", [ROOT, "-name", hash], { encoding: "utf8", timeout: 60_000 }).stdout.trim().split("\n").filter(Boolean);
  if (found.length === 0) { console.error(`  ✗ 前置不成立：磁盘上找不到本体 blob ${hash.slice(0, 12)}…`); process.exit(2); }
  const blobFile = found[0];
  rmSync(blobFile);
  console.log(`  已删磁盘本体：${blobFile}`);
  // **渲染路本身不变**：`render_region` 仍降级出图 + 告警点名缺失哈希。
  const preview = await call("render_region", { region: { x: 0, y: 0, w: 128, h: 128 } });
  check(preview.ok === true, "缺本体时 render_region 仍降级出图（渲染路不变）", JSON.stringify(preview).slice(0, 120));
  const warningsText = JSON.stringify(preview.warnings || preview);
  check(warningsText.includes(hash), "render_region 的告警点名缺失哈希", `告警 ${warningsText.slice(0, 120)}`);
  // **`export_png` 必须失败**：缺本体 ⇒ 不产图。
  const target = join(ROOT, "should_not_exist.png");
  const ex = await call("export_png", { path: target, width: 128, height: 128 });
  check(ex.ok === false, "缺本体时 export_png 必须失败", JSON.stringify(ex).slice(0, 160));
  check(ex.error_code === "reference_not_found", "失败码是 reference_not_found", `实测 ${ex.error_code}`);
  check(JSON.stringify(ex).includes(hash), "失败必须点名缺失的本体哈希", JSON.stringify(ex).slice(0, 160));
  check(!existsSync(target), "失败时目标 PNG 必须不存在（P2-12 交叉）", `目标 ${target}`);
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
console.log(failed === 0 ? "  结论：export_png 缺本体即失败 ✓" : `  结论：export_png 缺本体未失败 ✗（${failed} 条）`);
process.exit(failed === 0 ? 0 : 1);
